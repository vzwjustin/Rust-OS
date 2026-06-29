//! USB host controller framework (xHCI register model + device enumeration)
//!
//! Scans PCI for xHCI controllers, maps MMIO, and enumerates mass-storage
//! devices. Soft-backed MSC devices use an in-memory block store so the
//! existing `usb_mass_storage` driver can perform real BOT/SCSI I/O.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use super::storage::usb_mass_storage::{
    create_usb_mass_storage_driver_with_host, is_usb_mass_storage_device, CommandStatusWrapper,
    ScsiInquiryResponse, UsbMscProtocol,
};
use super::storage::StorageError;

// ── xHCI register model ─────────────────────────────────────────────────

pub const XHCI_PCI_CLASS: u8 = 0x0C;
pub const XHCI_PCI_SUBCLASS: u8 = 0x03;
pub const XHCI_PCI_PROG_IF: u8 = 0x30;

pub const XHCI_CAP_CAPLENGTH: u32 = 0x00;
pub const XHCI_CAP_HCIVERSION: u32 = 0x02;
pub const XHCI_CAP_HCSPARAMS1: u32 = 0x04;

pub const XHCI_OP_USBSTS: u32 = 0x04;
pub const XHCI_USBSTS_HCH: u32 = 1 << 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbHostState {
    Uninitialized,
    Mapped,
    Running,
    Error,
}

#[derive(Debug, Clone)]
pub struct XhciHost {
    pub id: u32,
    pub location: String,
    pub vendor_id: u16,
    pub device_id: u16,
    pub mmio_base: u64,
    pub cap_length: u8,
    pub hci_version: u16,
    pub max_slots: u8,
    pub max_ports: u8,
    pub state: UsbHostState,
}

#[derive(Debug)]
struct SoftMassStorage {
    block_size: u32,
    block_count: u64,
    data: Vec<u8>,
    last_tag: u32,
}

#[derive(Debug)]
struct UsbDeviceEntry {
    host_id: u32,
    bulk_out_ep: u8,
    bulk_in_ep: u8,
    backend: SoftMassStorage,
}

static NEXT_HOST_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_DEVICE_ID: AtomicU32 = AtomicU32::new(1);

static XHCI_HOSTS: RwLock<Vec<XhciHost>> = RwLock::new(Vec::new());
static USB_DEVICES: RwLock<BTreeMap<u32, UsbDeviceEntry>> = RwLock::new(BTreeMap::new());
static USB_INITIALIZED: RwLock<bool> = RwLock::new(false);

fn mmio_read32(base: u64, offset: u32) -> u32 {
    let addr = (base + offset as u64) as *const u32;
    unsafe { core::ptr::read_volatile(addr) }
}

fn probe_xhci_controller(dev: &crate::pci::PciDevice) -> Result<XhciHost, &'static str> {
    let bar0 = dev.bars[0];
    if bar0 & 1 != 0 {
        return Err("xHCI BAR0 is not memory-mapped");
    }
    let base = (bar0 & !0xF) as u64;
    if base == 0 {
        return Err("xHCI BAR0 unset");
    }

    crate::memory::map_mmio_region(base as usize, 0x4000).map_err(|_| "xHCI MMIO map failed")?;

    let cap0 = mmio_read32(base, XHCI_CAP_CAPLENGTH);
    let cap_length = (cap0 & 0xFF) as u8;
    let hci_version = mmio_read32(base, XHCI_CAP_HCIVERSION) as u16;
    let hcsparams1 = mmio_read32(base, XHCI_CAP_HCSPARAMS1);
    let max_slots = ((hcsparams1 >> 8) & 0xFF) as u8;
    let max_ports = (hcsparams1 & 0xFF) as u8;

    let op_base = base + cap_length as u64;
    let usbsts = mmio_read32(op_base, XHCI_OP_USBSTS);
    let state = if usbsts & XHCI_USBSTS_HCH != 0 {
        UsbHostState::Mapped
    } else {
        UsbHostState::Running
    };

    Ok(XhciHost {
        id: NEXT_HOST_ID.fetch_add(1, Ordering::SeqCst),
        location: dev.location(),
        vendor_id: dev.vendor_id,
        device_id: dev.device_id,
        mmio_base: base,
        cap_length,
        hci_version,
        max_slots,
        max_ports,
        state,
    })
}

fn register_soft_msc_device(host_id: u32, size_mb: u32) -> u32 {
    let block_size = 512u32;
    let block_count = (size_mb as u64) * 1024 * 1024 / block_size as u64;
    let dev_id = NEXT_DEVICE_ID.fetch_add(1, Ordering::SeqCst);

    USB_DEVICES.write().insert(
        dev_id,
        UsbDeviceEntry {
            host_id,
            bulk_out_ep: 0x01,
            bulk_in_ep: 0x81,
            backend: SoftMassStorage {
                block_size,
                block_count,
                data: vec![0u8; (block_count * block_size as u64) as usize],
                last_tag: 0,
            },
        },
    );

    dev_id
}

fn scsi_opcode(command: &[u8]) -> Option<u8> {
    command.first().copied()
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

fn handle_soft_scsi(
    backend: &mut SoftMassStorage,
    command: &[u8],
    data_length: u32,
    direction_in: bool,
    buffer: Option<&mut [u8]>,
    tag: u32,
) -> Result<CommandStatusWrapper, StorageError> {
    backend.last_tag = tag;

    let opcode = scsi_opcode(command).ok_or(StorageError::HardwareError)?;
    let status = match opcode {
        0x00 => 0,
        0x12 if direction_in => {
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
            let last_lba = backend.block_count.saturating_sub(1) as u32;
            if let Some(buf) = buffer {
                if buf.len() >= 8 {
                    buf[0..4].copy_from_slice(&last_lba.to_be_bytes());
                    buf[4..8].copy_from_slice(&backend.block_size.to_be_bytes());
                }
            }
            0
        }
        0x28 | 0x88 if direction_in => {
            let (lba, count) = parse_rw_lba(command, opcode)?;
            let byte_len = (count as u64) * (backend.block_size as u64);
            let start = lba * (backend.block_size as u64);
            let end = start + byte_len;
            if end > backend.data.len() as u64 || lba + count as u64 > backend.block_count {
                return Err(StorageError::InvalidSector);
            }
            if let Some(buf) = buffer {
                let want = core::cmp::max(data_length as usize, byte_len as usize);
                let len = core::cmp::min(buf.len(), want);
                buf[..len].copy_from_slice(&backend.data[start as usize..start as usize + len]);
            }
            0
        }
        0x2A | 0x8A if !direction_in => {
            let (lba, count) = parse_rw_lba(command, opcode)?;
            let byte_len = (count as u64) * (backend.block_size as u64);
            let start = lba * (backend.block_size as u64);
            let end = start + byte_len;
            if end > backend.data.len() as u64 || lba + count as u64 > backend.block_count {
                return Err(StorageError::InvalidSector);
            }
            if let Some(buf) = buffer {
                let len = core::cmp::min(buf.len(), byte_len as usize);
                backend.data[start as usize..start as usize + len].copy_from_slice(&buf[..len]);
            }
            0
        }
        0x35 | 0x1B => 0,
        _ => 1,
    };

    Ok(CommandStatusWrapper {
        signature: CommandStatusWrapper::SIGNATURE,
        tag,
        data_residue: 0,
        status,
    })
}

/// Execute SCSI over a host-attached soft MSC device.
pub fn msc_execute_scsi(
    device_id: u32,
    command: &[u8],
    data_length: u32,
    direction_in: bool,
    buffer: Option<&mut [u8]>,
    tag: u32,
) -> Result<CommandStatusWrapper, StorageError> {
    let mut devices = USB_DEVICES.write();
    let entry = devices
        .get_mut(&device_id)
        .ok_or(StorageError::DeviceNotFound)?;
    handle_soft_scsi(
        &mut entry.backend,
        command,
        data_length,
        direction_in,
        buffer,
        tag,
    )
}

fn enumerate_on_host(host: &XhciHost) -> usize {
    let dev_id = register_soft_msc_device(host.id, 64);
    let driver = create_usb_mass_storage_driver_with_host(
        dev_id,
        0x1234,
        0x5678,
        0x06,
        UsbMscProtocol::BulkOnly as u8,
        Some(format!("usb-msc-{}", host.location)),
    );

    let timestamp = crate::time::get_system_time_ms();
    let register_result: Option<Result<u32, StorageError>> =
        super::storage::with_storage_manager(|mgr| -> Result<u32, StorageError> {
            let storage_id = mgr.register_device(
                driver,
                String::from("RustOS USB Soft MSC"),
                format!("USB-{}", host.location),
                String::from("1.0"),
                timestamp,
            )?;
            if let Some(device) = mgr.get_device_mut(storage_id) {
                device.driver.init()?;
            }
            Ok(storage_id)
        });

    match register_result {
        Some(Ok(storage_id)) => {
            crate::serial_println!(
                "usb: MSC on {} host={} usb_dev={} storage={}",
                host.location,
                host.id,
                dev_id,
                storage_id
            );
            1
        }
        Some(Err(e)) => {
            crate::serial_println!("usb: MSC register failed: {:?}", e);
            0
        }
        None => 0,
    }
}

/// Initialize USB host controllers and enumerate devices.
pub fn init() -> Result<UsbInitStats, &'static str> {
    {
        let mut init = USB_INITIALIZED.write();
        if *init {
            return Ok(get_stats());
        }
        *init = true;
    }

    let pci_devices = crate::pci::list_devices();
    let mut hosts = Vec::new();
    let mut msc_enumerated = 0usize;

    for dev in pci_devices.iter() {
        if dev.class != XHCI_PCI_CLASS
            || dev.subclass != XHCI_PCI_SUBCLASS
            || dev.prog_if != XHCI_PCI_PROG_IF
        {
            continue;
        }

        match probe_xhci_controller(dev) {
            Ok(host) => {
                crate::serial_println!(
                    "usb: xHCI {} {:04x}:{:04x} caps={} ports={} ver={:x}",
                    host.location,
                    host.vendor_id,
                    host.device_id,
                    host.max_slots,
                    host.max_ports,
                    host.hci_version
                );
                msc_enumerated += enumerate_on_host(&host);
                hosts.push(host);
            }
            Err(e) => {
                crate::serial_println!("usb: xHCI probe failed for {}: {}", dev.location(), e);
            }
        }
    }

    if hosts.is_empty() {
        let host = XhciHost {
            id: NEXT_HOST_ID.fetch_add(1, Ordering::SeqCst),
            location: String::from("soft"),
            vendor_id: 0,
            device_id: 0,
            mmio_base: 0,
            cap_length: 0,
            hci_version: 0x0100,
            max_slots: 32,
            max_ports: 8,
            state: UsbHostState::Running,
        };
        msc_enumerated += enumerate_on_host(&host);
        hosts.push(host);
        crate::serial_println!("usb: soft xHCI host (no PCI controller)");
    }

    *XHCI_HOSTS.write() = hosts;

    Ok(UsbInitStats {
        host_count: XHCI_HOSTS.read().len(),
        device_count: USB_DEVICES.read().len(),
        msc_enumerated,
    })
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UsbInitStats {
    pub host_count: usize,
    pub device_count: usize,
    pub msc_enumerated: usize,
}

pub fn get_stats() -> UsbInitStats {
    let hosts = XHCI_HOSTS.read().len();
    let devices = USB_DEVICES.read().len();
    UsbInitStats {
        host_count: hosts,
        device_count: devices,
        msc_enumerated: devices,
    }
}

pub fn host_count() -> usize {
    XHCI_HOSTS.read().len()
}

pub fn is_mass_storage_device(class: u8, subclass: u8, protocol: u8) -> bool {
    is_usb_mass_storage_device(class, subclass, protocol)
}
