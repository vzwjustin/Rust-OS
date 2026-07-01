//! Device-side virtual USB peripherals.
//!
//! A `VirtualDevice` answers the control/bulk/interrupt transfers that the
//! xHCI model dispatches when its doorbell is rung, so the whole host stack
//! can be exercised end to end with no hardware. Two peripherals are
//! provided: a boot-protocol HID keyboard and a Bulk-Only-Transport flash
//! disk. The disk shares its SCSI engine (`SoftDisk`) with the legacy
//! `msc_execute_scsi` path so there is a single source of truth for SCSI.

use alloc::vec;
use alloc::vec::Vec;

use super::descriptor::{self, class};
use super::hcd::{
    SetupPacket, TransferDirection, TransferResult, UsbSpeed, DESC_CONFIGURATION, DESC_DEVICE,
    REQ_GET_DESCRIPTOR, REQ_SET_ADDRESS, REQ_SET_CONFIGURATION,
};
use crate::drivers::storage::usb_mass_storage::{CommandStatusWrapper, ScsiInquiryResponse};
use crate::drivers::storage::StorageError;

/// A device the controller can dispatch transfers to.
pub trait VirtualDevice: Send + Sync {
    /// Signalling speed of the device.
    fn speed(&self) -> UsbSpeed;

    /// Handle a control transfer on endpoint 0.
    fn control(&mut self, setup: SetupPacket, data: Option<&mut [u8]>) -> TransferResult;

    /// Handle a bulk transfer on `endpoint`.
    fn bulk(&mut self, endpoint: u8, dir: TransferDirection, buf: &mut [u8]) -> TransferResult;

    /// Handle an interrupt-IN transfer on `endpoint`.
    fn interrupt(&mut self, endpoint: u8, buf: &mut [u8]) -> TransferResult;
}

/// Copy a descriptor blob into the host buffer honouring `wLength`, returning
/// a short-packet-aware transfer result.
fn respond_descriptor(blob: &[u8], setup: SetupPacket, data: Option<&mut [u8]>) -> TransferResult {
    let want = core::cmp::min(setup.length as usize, blob.len());
    if let Some(buf) = data {
        let n = core::cmp::min(buf.len(), want);
        buf[..n].copy_from_slice(&blob[..n]);
        if n < setup.length as usize {
            TransferResult::short(n)
        } else {
            TransferResult::ok(n)
        }
    } else {
        TransferResult::ok(0)
    }
}

const HID_REQ_GET_REPORT: u8 = 0x01;
const HID_REQ_SET_IDLE: u8 = 0x0A;
const HID_REQ_SET_PROTOCOL: u8 = 0x0B;

// ── Virtual HID boot keyboard ───────────────────────────────────────────

/// Boot-protocol HID keyboard that replays a short canned key sequence on its
/// interrupt-IN endpoint.
pub struct VirtualHidKeyboard {
    address: u8,
    configured: bool,
    boot_protocol: bool,
    idle_rate: u8,
    interrupt_in_ep: u8,
    /// Pre-loaded 8-byte boot reports, popped one per interrupt-IN.
    reports: Vec<[u8; 8]>,
    cursor: usize,
}

impl VirtualHidKeyboard {
    pub fn new(interrupt_in_ep: u8) -> Self {
        // Type "AB" then release: modifier=0, reserved=0, keycodes follow.
        // HID usage: 0x04='a', 0x05='b'.
        let reports = vec![
            [0, 0, 0x04, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0x05, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0],
        ];
        VirtualHidKeyboard {
            address: 0,
            configured: false,
            boot_protocol: true,
            idle_rate: 0,
            interrupt_in_ep,
            reports,
            cursor: 0,
        }
    }

    fn device_descriptor(&self) -> descriptor::DeviceDescriptor {
        descriptor::DeviceDescriptor {
            length: descriptor::DeviceDescriptor::SIZE as u8,
            descriptor_type: DESC_DEVICE,
            usb_version: 0x0200,
            device_class: class::PER_INTERFACE,
            device_subclass: 0,
            device_protocol: 0,
            max_packet_size0: 8,
            vendor_id: 0x046D,
            product_id: 0xC31C,
            device_version: 0x0100,
            manufacturer_index: 0,
            product_index: 0,
            serial_index: 0,
            num_configurations: 1,
        }
    }

    fn configuration_blob(&self) -> Vec<u8> {
        // config(9) + interface(9) + HID class desc(9) + endpoint(7) = 34
        let total: u16 = 9 + 9 + 9 + 7;
        let mut b = Vec::with_capacity(total as usize);
        // Configuration descriptor.
        b.extend_from_slice(&[
            9,
            DESC_CONFIGURATION,
            total as u8,
            (total >> 8) as u8,
            1, // num interfaces
            1, // configuration value
            0, // index
            0xA0,
            50,
        ]);
        // Interface descriptor: HID, boot subclass, keyboard protocol.
        b.extend_from_slice(&[9, 0x04, 0, 0, 1, class::HID, 0x01, 0x01, 0]);
        // HID class descriptor (9 bytes, opaque to our parser).
        b.extend_from_slice(&[9, 0x21, 0x11, 0x01, 0x00, 1, 0x22, 63, 0]);
        // Endpoint descriptor: interrupt IN.
        b.extend_from_slice(&[
            7,
            0x05,
            self.interrupt_in_ep,
            0x03, // interrupt
            8,
            0,
            10, // bInterval
        ]);
        b
    }
}

impl VirtualDevice for VirtualHidKeyboard {
    fn speed(&self) -> UsbSpeed {
        UsbSpeed::Low
    }

    fn control(&mut self, setup: SetupPacket, data: Option<&mut [u8]>) -> TransferResult {
        match setup.request {
            REQ_SET_ADDRESS => {
                self.address = setup.value as u8;
                TransferResult::ok(0)
            }
            REQ_SET_CONFIGURATION => {
                self.configured = setup.value != 0;
                TransferResult::ok(0)
            }
            REQ_GET_DESCRIPTOR => {
                let desc_type = (setup.value >> 8) as u8;
                match desc_type {
                    DESC_DEVICE => {
                        respond_descriptor(&self.device_descriptor().to_bytes(), setup, data)
                    }
                    DESC_CONFIGURATION => {
                        respond_descriptor(&self.configuration_blob(), setup, data)
                    }
                    _ => TransferResult::stall(),
                }
            }
            HID_REQ_SET_PROTOCOL => {
                self.boot_protocol = setup.value == 0;
                TransferResult::ok(0)
            }
            HID_REQ_SET_IDLE => {
                self.idle_rate = (setup.value >> 8) as u8;
                TransferResult::ok(0)
            }
            HID_REQ_GET_REPORT => {
                let report = self.reports.get(self.cursor).copied().unwrap_or([0u8; 8]);
                if let Some(buf) = data {
                    let n = core::cmp::min(buf.len(), report.len());
                    buf[..n].copy_from_slice(&report[..n]);
                    TransferResult::ok(n)
                } else {
                    TransferResult::ok(0)
                }
            }
            _ => TransferResult::ok(0),
        }
    }

    fn bulk(&mut self, _endpoint: u8, _dir: TransferDirection, _buf: &mut [u8]) -> TransferResult {
        TransferResult::stall()
    }

    fn interrupt(&mut self, endpoint: u8, buf: &mut [u8]) -> TransferResult {
        if endpoint != self.interrupt_in_ep || !self.configured || !self.boot_protocol {
            return TransferResult::stall();
        }
        if self.cursor >= self.reports.len() {
            // NAK modelled as a zero-length transfer (no new report).
            return TransferResult::ok(0);
        }
        let report = self.reports[self.cursor];
        self.cursor += 1;
        let n = core::cmp::min(buf.len(), report.len());
        buf[..n].copy_from_slice(&report[..n]);
        TransferResult::ok(n)
    }
}

// ── Virtual HID boot mouse ──────────────────────────────────────────────

/// Boot-protocol HID mouse with a small deterministic report stream.
pub struct VirtualHidMouse {
    address: u8,
    configured: bool,
    boot_protocol: bool,
    idle_rate: u8,
    interrupt_in_ep: u8,
    reports: Vec<[u8; 4]>,
    cursor: usize,
}

impl VirtualHidMouse {
    pub fn new(interrupt_in_ep: u8) -> Self {
        let reports = vec![
            [0, 8, 0, 0], // move right
            [1, 0, 0, 0], // left button down
            [0, 0, 0, 0], // release
            [0, 0, 0, 1], // wheel up
        ];
        Self {
            address: 0,
            configured: false,
            boot_protocol: true,
            idle_rate: 0,
            interrupt_in_ep,
            reports,
            cursor: 0,
        }
    }

    fn device_descriptor(&self) -> descriptor::DeviceDescriptor {
        descriptor::DeviceDescriptor {
            length: descriptor::DeviceDescriptor::SIZE as u8,
            descriptor_type: DESC_DEVICE,
            usb_version: 0x0200,
            device_class: class::PER_INTERFACE,
            device_subclass: 0,
            device_protocol: 0,
            max_packet_size0: 8,
            vendor_id: 0x046D,
            product_id: 0xC077,
            device_version: 0x0100,
            manufacturer_index: 0,
            product_index: 0,
            serial_index: 0,
            num_configurations: 1,
        }
    }

    fn configuration_blob(&self) -> Vec<u8> {
        let total: u16 = 9 + 9 + 9 + 7;
        let mut b = Vec::with_capacity(total as usize);
        b.extend_from_slice(&[
            9,
            DESC_CONFIGURATION,
            total as u8,
            (total >> 8) as u8,
            1,
            1,
            0,
            0xA0,
            50,
        ]);
        // Interface descriptor: HID, boot subclass, mouse protocol.
        b.extend_from_slice(&[9, 0x04, 0, 0, 1, class::HID, 0x01, 0x02, 0]);
        // HID class descriptor with a compact boot-mouse report descriptor.
        b.extend_from_slice(&[9, 0x21, 0x11, 0x01, 0x00, 1, 0x22, 52, 0]);
        b.extend_from_slice(&[7, 0x05, self.interrupt_in_ep, 0x03, 4, 0, 10]);
        b
    }
}

impl VirtualDevice for VirtualHidMouse {
    fn speed(&self) -> UsbSpeed {
        UsbSpeed::Low
    }

    fn control(&mut self, setup: SetupPacket, data: Option<&mut [u8]>) -> TransferResult {
        match setup.request {
            REQ_SET_ADDRESS => {
                self.address = setup.value as u8;
                TransferResult::ok(0)
            }
            REQ_SET_CONFIGURATION => {
                self.configured = setup.value != 0;
                TransferResult::ok(0)
            }
            REQ_GET_DESCRIPTOR => {
                let desc_type = (setup.value >> 8) as u8;
                match desc_type {
                    DESC_DEVICE => {
                        respond_descriptor(&self.device_descriptor().to_bytes(), setup, data)
                    }
                    DESC_CONFIGURATION => {
                        respond_descriptor(&self.configuration_blob(), setup, data)
                    }
                    _ => TransferResult::stall(),
                }
            }
            HID_REQ_SET_PROTOCOL => {
                self.boot_protocol = setup.value == 0;
                TransferResult::ok(0)
            }
            HID_REQ_SET_IDLE => {
                self.idle_rate = (setup.value >> 8) as u8;
                TransferResult::ok(0)
            }
            HID_REQ_GET_REPORT => {
                let report = self.reports.get(self.cursor).copied().unwrap_or([0u8; 4]);
                if let Some(buf) = data {
                    let n = core::cmp::min(buf.len(), report.len());
                    buf[..n].copy_from_slice(&report[..n]);
                    TransferResult::ok(n)
                } else {
                    TransferResult::ok(0)
                }
            }
            _ => TransferResult::ok(0),
        }
    }

    fn bulk(&mut self, _endpoint: u8, _dir: TransferDirection, _buf: &mut [u8]) -> TransferResult {
        TransferResult::stall()
    }

    fn interrupt(&mut self, endpoint: u8, buf: &mut [u8]) -> TransferResult {
        if endpoint != self.interrupt_in_ep || !self.configured || !self.boot_protocol {
            return TransferResult::stall();
        }
        if self.cursor >= self.reports.len() {
            return TransferResult::ok(0);
        }
        let report = self.reports[self.cursor];
        self.cursor += 1;
        let n = core::cmp::min(buf.len(), report.len());
        buf[..n].copy_from_slice(&report[..n]);
        TransferResult::ok(n)
    }
}

// ── Shared soft SCSI engine ─────────────────────────────────────────────

/// In-memory block device with a small SCSI command interpreter. Used both by
/// the BOT virtual device and by the legacy `msc_execute_scsi` entry point.
#[derive(Debug)]
pub struct SoftDisk {
    pub block_size: u32,
    pub block_count: u64,
    pub data: Vec<u8>,
}

impl SoftDisk {
    pub fn new(size_mb: u32) -> Self {
        let block_size = 512u32;
        let block_count = (size_mb as u64) * 1024 * 1024 / block_size as u64;
        SoftDisk {
            block_size,
            block_count,
            data: vec![0u8; (block_count * block_size as u64) as usize],
        }
    }

    fn parse_rw_lba(command: &[u8], opcode: u8) -> Result<(u64, u32), StorageError> {
        if opcode == 0x28 || opcode == 0x2A {
            if command.len() < 10 {
                return Err(StorageError::HardwareError);
            }
            let lba = u32::from_be_bytes([command[2], command[3], command[4], command[5]]) as u64;
            let count = u16::from_be_bytes([command[7], command[8]]) as u32;
            Ok((lba, count))
        } else if opcode == 0x88 || opcode == 0x8A {
            if command.len() < 14 {
                return Err(StorageError::HardwareError);
            }
            let lba = u64::from_be_bytes([
                command[2], command[3], command[4], command[5], command[6], command[7], command[8],
                command[9],
            ]);
            let count = u32::from_be_bytes([command[10], command[11], command[12], command[13]]);
            Ok((lba, count))
        } else {
            Err(StorageError::HardwareError)
        }
    }

    /// Execute one SCSI command. Returns the CSW that BOT would report.
    pub fn execute_scsi(
        &mut self,
        command: &[u8],
        data_length: u32,
        direction_in: bool,
        buffer: Option<&mut [u8]>,
        tag: u32,
    ) -> Result<CommandStatusWrapper, StorageError> {
        let opcode = command
            .first()
            .copied()
            .ok_or(StorageError::HardwareError)?;
        let status = match opcode {
            0x00 => 0, // TEST UNIT READY
            0x12 if direction_in => {
                // INQUIRY
                let response = ScsiInquiryResponse {
                    peripheral: 0x00,
                    removable: 0x80,
                    version: 0x04,
                    response_format: 0x02,
                    additional_length: 31,
                    flags: [0; 3],
                    vendor_id: *b"RustOS  ",
                    product_id: *b"USB Soft MSC    ",
                    product_revision: *b"1.0 ",
                };
                if let Some(buf) = buffer {
                    let bytes = unsafe {
                        core::slice::from_raw_parts(
                            &response as *const ScsiInquiryResponse as *const u8,
                            core::mem::size_of::<ScsiInquiryResponse>(),
                        )
                    };
                    let len = core::cmp::min(buf.len(), bytes.len());
                    buf[..len].copy_from_slice(&bytes[..len]);
                }
                0
            }
            0x25 if direction_in => {
                // READ CAPACITY(10)
                let last_lba = self.block_count.saturating_sub(1) as u32;
                if let Some(buf) = buffer {
                    if buf.len() >= 8 {
                        buf[0..4].copy_from_slice(&last_lba.to_be_bytes());
                        buf[4..8].copy_from_slice(&self.block_size.to_be_bytes());
                    }
                }
                0
            }
            0x28 | 0x88 if direction_in => {
                // READ(10) / READ(16)
                let (lba, count) = Self::parse_rw_lba(command, opcode)?;
                let byte_len = (count as u64) * (self.block_size as u64);
                let start = lba * (self.block_size as u64);
                let end = start + byte_len;
                if end > self.data.len() as u64 || lba + count as u64 > self.block_count {
                    return Err(StorageError::InvalidSector);
                }
                if let Some(buf) = buffer {
                    let want = core::cmp::max(data_length as usize, byte_len as usize);
                    let len = core::cmp::min(buf.len(), want);
                    let len = core::cmp::min(len, byte_len as usize);
                    buf[..len].copy_from_slice(&self.data[start as usize..start as usize + len]);
                }
                0
            }
            0x2A | 0x8A if !direction_in => {
                // WRITE(10) / WRITE(16)
                let (lba, count) = Self::parse_rw_lba(command, opcode)?;
                let byte_len = (count as u64) * (self.block_size as u64);
                let start = lba * (self.block_size as u64);
                let end = start + byte_len;
                if end > self.data.len() as u64 || lba + count as u64 > self.block_count {
                    return Err(StorageError::InvalidSector);
                }
                if let Some(buf) = buffer {
                    let len = core::cmp::min(buf.len(), byte_len as usize);
                    self.data[start as usize..start as usize + len].copy_from_slice(&buf[..len]);
                }
                0
            }
            0x35 | 0x1B => 0, // SYNCHRONIZE CACHE / START STOP UNIT
            _ => 1,           // unsupported -> command failed
        };

        Ok(CommandStatusWrapper {
            signature: CommandStatusWrapper::SIGNATURE,
            tag,
            data_residue: 0,
            status,
        })
    }
}

// ── Virtual Bulk-Only-Transport flash disk ──────────────────────────────

const CBW_SIGNATURE: u32 = 0x4342_5355; // 'USBC'
const CBW_LEN: usize = 31;
const CSW_LEN: usize = 13;

#[derive(Debug)]
enum BotPhase {
    /// Waiting for the next Command Block Wrapper.
    Command,
    /// Device→host data pending; bytes carries the prepared payload.
    DataIn { payload: Vec<u8>, offset: usize },
    /// Host→device data expected; buffer accumulates the write payload.
    DataOut {
        command: Vec<u8>,
        buffer: Vec<u8>,
        filled: usize,
    },
    /// Command/data complete; next IN delivers the Command Status Wrapper.
    Status,
}

/// BOT mass-storage device wrapping a `SoftDisk`.
pub struct VirtualBotDisk {
    address: u8,
    configured: bool,
    bulk_in_ep: u8,
    bulk_out_ep: u8,
    disk: SoftDisk,
    phase: BotPhase,
    pending_csw: CommandStatusWrapper,
}

impl VirtualBotDisk {
    pub fn new(size_mb: u32, bulk_in_ep: u8, bulk_out_ep: u8) -> Self {
        VirtualBotDisk {
            address: 0,
            configured: false,
            bulk_in_ep,
            bulk_out_ep,
            disk: SoftDisk::new(size_mb),
            phase: BotPhase::Command,
            pending_csw: CommandStatusWrapper {
                signature: CommandStatusWrapper::SIGNATURE,
                tag: 0,
                data_residue: 0,
                status: 0,
            },
        }
    }

    fn device_descriptor(&self) -> descriptor::DeviceDescriptor {
        descriptor::DeviceDescriptor {
            length: descriptor::DeviceDescriptor::SIZE as u8,
            descriptor_type: DESC_DEVICE,
            usb_version: 0x0200,
            device_class: class::PER_INTERFACE,
            device_subclass: 0,
            device_protocol: 0,
            max_packet_size0: 64,
            vendor_id: 0x0781,
            product_id: 0x5567,
            device_version: 0x0100,
            manufacturer_index: 0,
            product_index: 0,
            serial_index: 0,
            num_configurations: 1,
        }
    }

    fn configuration_blob(&self) -> Vec<u8> {
        // config(9) + interface(9) + endpoint IN(7) + endpoint OUT(7) = 32
        let total: u16 = 9 + 9 + 7 + 7;
        let mut b = Vec::with_capacity(total as usize);
        b.extend_from_slice(&[
            9,
            DESC_CONFIGURATION,
            total as u8,
            (total >> 8) as u8,
            1,
            1,
            0,
            0x80,
            100,
        ]);
        // Interface: Mass Storage, SCSI transparent (0x06), Bulk-Only (0x50).
        b.extend_from_slice(&[9, 0x04, 0, 0, 2, class::MASS_STORAGE, 0x06, 0x50, 0]);
        // Bulk IN endpoint.
        b.extend_from_slice(&[7, 0x05, self.bulk_in_ep, 0x02, 0x00, 0x02, 0]);
        // Bulk OUT endpoint.
        b.extend_from_slice(&[7, 0x05, self.bulk_out_ep, 0x02, 0x00, 0x02, 0]);
        b
    }

    /// Decode a CBW and arrange the next phase.
    fn accept_cbw(&mut self, cbw: &[u8]) -> TransferResult {
        if cbw.len() < CBW_LEN {
            return TransferResult::stall();
        }
        let signature = u32::from_le_bytes([cbw[0], cbw[1], cbw[2], cbw[3]]);
        if signature != CBW_SIGNATURE {
            return TransferResult::stall();
        }
        let tag = u32::from_le_bytes([cbw[4], cbw[5], cbw[6], cbw[7]]);
        let data_len = u32::from_le_bytes([cbw[8], cbw[9], cbw[10], cbw[11]]);
        let dir_in = cbw[12] & 0x80 != 0;
        let cb_len = (cbw[14] & 0x1F) as usize;
        let command = cbw[15..15 + cb_len.min(16)].to_vec();

        if data_len == 0 {
            // No data stage.
            self.pending_csw = self
                .disk
                .execute_scsi(&command, 0, false, None, tag)
                .unwrap_or_else(|_| Self::failed_csw(tag));
            self.phase = BotPhase::Status;
            return TransferResult::ok(cbw.len());
        }

        if dir_in {
            let mut payload = vec![0u8; data_len as usize];
            self.pending_csw = self
                .disk
                .execute_scsi(&command, data_len, true, Some(&mut payload), tag)
                .unwrap_or_else(|_| Self::failed_csw(tag));
            self.phase = BotPhase::DataIn { payload, offset: 0 };
        } else {
            self.phase = BotPhase::DataOut {
                command,
                buffer: vec![0u8; data_len as usize],
                filled: 0,
            };
            self.pending_csw = CommandStatusWrapper {
                signature: CommandStatusWrapper::SIGNATURE,
                tag,
                data_residue: 0,
                status: 0,
            };
        }
        TransferResult::ok(cbw.len())
    }

    fn failed_csw(tag: u32) -> CommandStatusWrapper {
        CommandStatusWrapper {
            signature: CommandStatusWrapper::SIGNATURE,
            tag,
            data_residue: 0,
            status: 1,
        }
    }

    fn csw_bytes(&self) -> [u8; CSW_LEN] {
        let mut b = [0u8; CSW_LEN];
        b[0..4].copy_from_slice(&self.pending_csw.signature.to_le_bytes());
        b[4..8].copy_from_slice(&self.pending_csw.tag.to_le_bytes());
        b[8..12].copy_from_slice(&self.pending_csw.data_residue.to_le_bytes());
        b[12] = self.pending_csw.status;
        b
    }
}

impl VirtualDevice for VirtualBotDisk {
    fn speed(&self) -> UsbSpeed {
        UsbSpeed::High
    }

    fn control(&mut self, setup: SetupPacket, data: Option<&mut [u8]>) -> TransferResult {
        match setup.request {
            REQ_SET_ADDRESS => {
                self.address = setup.value as u8;
                TransferResult::ok(0)
            }
            REQ_SET_CONFIGURATION => {
                self.configured = setup.value != 0;
                TransferResult::ok(0)
            }
            REQ_GET_DESCRIPTOR => {
                let desc_type = (setup.value >> 8) as u8;
                match desc_type {
                    DESC_DEVICE => {
                        respond_descriptor(&self.device_descriptor().to_bytes(), setup, data)
                    }
                    DESC_CONFIGURATION => {
                        respond_descriptor(&self.configuration_blob(), setup, data)
                    }
                    _ => TransferResult::stall(),
                }
            }
            // 0xFF/0xFE: Bulk-Only Mass Storage Reset / Get Max LUN.
            0xFE => {
                if let Some(buf) = data {
                    if !buf.is_empty() {
                        buf[0] = 0; // single LUN
                    }
                }
                TransferResult::ok(1)
            }
            _ => TransferResult::ok(0),
        }
    }

    fn bulk(&mut self, endpoint: u8, dir: TransferDirection, buf: &mut [u8]) -> TransferResult {
        if !self.configured {
            return TransferResult::stall();
        }
        match dir {
            TransferDirection::Out if endpoint == self.bulk_out_ep => {
                // Take ownership of the phase to avoid borrowing `self` twice.
                let phase = core::mem::replace(&mut self.phase, BotPhase::Command);
                match phase {
                    // accept_cbw resets self.phase itself.
                    BotPhase::Command => self.accept_cbw(buf),
                    BotPhase::DataOut {
                        command,
                        mut buffer,
                        mut filled,
                    } => {
                        let n = core::cmp::min(buf.len(), buffer.len() - filled);
                        buffer[filled..filled + n].copy_from_slice(&buf[..n]);
                        filled += n;
                        if filled >= buffer.len() {
                            let tag = self.pending_csw.tag;
                            self.pending_csw = self
                                .disk
                                .execute_scsi(
                                    &command,
                                    buffer.len() as u32,
                                    false,
                                    Some(&mut buffer),
                                    tag,
                                )
                                .unwrap_or_else(|_| Self::failed_csw(tag));
                            self.phase = BotPhase::Status;
                        } else {
                            self.phase = BotPhase::DataOut {
                                command,
                                buffer,
                                filled,
                            };
                        }
                        TransferResult::ok(n)
                    }
                    other => {
                        self.phase = other;
                        TransferResult::stall()
                    }
                }
            }
            TransferDirection::In if endpoint == self.bulk_in_ep => {
                let phase = core::mem::replace(&mut self.phase, BotPhase::Command);
                match phase {
                    BotPhase::DataIn {
                        payload,
                        mut offset,
                    } => {
                        let n = core::cmp::min(buf.len(), payload.len() - offset);
                        buf[..n].copy_from_slice(&payload[offset..offset + n]);
                        offset += n;
                        if offset >= payload.len() {
                            self.phase = BotPhase::Status;
                        } else {
                            self.phase = BotPhase::DataIn { payload, offset };
                        }
                        TransferResult::ok(n)
                    }
                    // CSW delivered; self.phase already reset to Command.
                    BotPhase::Status => {
                        let csw = self.csw_bytes();
                        let n = core::cmp::min(buf.len(), csw.len());
                        buf[..n].copy_from_slice(&csw[..n]);
                        TransferResult::ok(n)
                    }
                    other => {
                        self.phase = other;
                        TransferResult::stall()
                    }
                }
            }
            _ => TransferResult::stall(),
        }
    }

    fn interrupt(&mut self, _endpoint: u8, _buf: &mut [u8]) -> TransferResult {
        TransferResult::stall()
    }
}
