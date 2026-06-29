//! # USB Mass Storage Driver
//!
//! USB Mass Storage Class (MSC) driver for USB storage devices like
//! USB flash drives, external hard drives, and USB card readers.

use super::{
    StorageCapabilities, StorageDeviceState, StorageDeviceType, StorageDriver, StorageError,
    StorageStats,
};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// USB Mass Storage Class codes
pub const USB_CLASS_MASS_STORAGE: u8 = 0x08;

/// USB Mass Storage Subclass codes
#[repr(u8)]
pub enum UsbMscSubclass {
    /// Reduced Block Commands (RBC)
    Rbc = 0x01,
    /// ATAPI/MMC-5 (typically CD/DVD)
    Atapi = 0x02,
    /// QIC-157 (tape)
    Qic157 = 0x03,
    /// UFI (floppy)
    Ufi = 0x04,
    /// SFF-8070i (floppy)
    Sff8070i = 0x05,
    /// SCSI Transparent Command Set
    ScsiTransparent = 0x06,
    /// LSD FS (file system)
    LsdFs = 0x07,
    /// IEEE 1667
    Ieee1667 = 0x08,
}

/// USB Mass Storage Protocol codes
#[repr(u8)]
pub enum UsbMscProtocol {
    /// Control/Bulk/Interrupt (CBI) with command completion interrupt
    CbiCci = 0x00,
    /// Control/Bulk/Interrupt (CBI) without command completion interrupt
    Cbi = 0x01,
    /// Bulk-Only Transport (BOT)
    BulkOnly = 0x50,
    /// USB Attached SCSI (UAS)
    Uas = 0x62,
}

/// Command Block Wrapper (CBW) for Bulk-Only Transport
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CommandBlockWrapper {
    /// Signature: 'USBC' (0x43425355)
    pub signature: u32,
    /// Transaction ID
    pub tag: u32,
    /// Data transfer length
    pub data_transfer_length: u32,
    /// Flags (bit 7: direction, 0=OUT, 1=IN)
    pub flags: u8,
    /// LUN (Logical Unit Number)
    pub lun: u8,
    /// Command Block Length (1-16)
    pub cb_length: u8,
    /// Command Block
    pub command_block: [u8; 16],
}

impl CommandBlockWrapper {
    const SIGNATURE: u32 = 0x43425355; // 'USBC'

    pub fn new(tag: u32, data_length: u32, direction_in: bool, lun: u8, command: &[u8]) -> Self {
        let mut cbw = Self {
            signature: Self::SIGNATURE,
            tag,
            data_transfer_length: data_length,
            flags: if direction_in { 0x80 } else { 0x00 },
            lun,
            cb_length: command.len() as u8,
            command_block: [0; 16],
        };

        // Copy command into command block
        let copy_len = core::cmp::min(command.len(), 16);
        cbw.command_block[..copy_len].copy_from_slice(&command[..copy_len]);

        cbw
    }
}

/// Command Status Wrapper (CSW) for Bulk-Only Transport
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CommandStatusWrapper {
    /// Signature: 'USBS' (0x53425355)
    pub signature: u32,
    /// Transaction ID (should match CBW tag)
    pub tag: u32,
    /// Data residue
    pub data_residue: u32,
    /// Status (0=success, 1=fail, 2=phase error)
    pub status: u8,
}

impl CommandStatusWrapper {
    pub const SIGNATURE: u32 = 0x53425355; // 'USBS'

    pub fn is_valid(&self) -> bool {
        self.signature == Self::SIGNATURE
    }

    pub fn is_success(&self) -> bool {
        self.status == 0
    }
}

/// SCSI commands commonly used with USB Mass Storage
#[repr(u8)]
pub enum ScsiCommand {
    /// Test Unit Ready
    TestUnitReady = 0x00,
    /// Request Sense
    RequestSense = 0x03,
    /// Inquiry
    Inquiry = 0x12,
    /// Mode Sense (6)
    ModeSense6 = 0x1A,
    /// Start Stop Unit
    StartStopUnit = 0x1B,
    /// Prevent/Allow Medium Removal
    PreventAllowMediumRemoval = 0x1E,
    /// Read Format Capacities
    ReadFormatCapacities = 0x23,
    /// Read Capacity (10)
    ReadCapacity10 = 0x25,
    /// Read (10)
    Read10 = 0x28,
    /// Write (10)
    Write10 = 0x2A,
    /// Verify (10)
    Verify10 = 0x2F,
    /// Synchronize Cache
    SynchronizeCache = 0x35,
    /// Read Capacity (16)
    ReadCapacity16 = 0x9E,
    /// Read (16)
    Read16 = 0x88,
    /// Write (16)
    Write16 = 0x8A,
}

/// SCSI Inquiry response
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ScsiInquiryResponse {
    /// Peripheral qualifier and device type
    pub peripheral: u8,
    /// Removable medium bit and reserved
    pub removable: u8,
    /// Version
    pub version: u8,
    /// Response data format
    pub response_format: u8,
    /// Additional length
    pub additional_length: u8,
    /// Flags
    pub flags: [u8; 3],
    /// Vendor identification (8 bytes)
    pub vendor_id: [u8; 8],
    /// Product identification (16 bytes)
    pub product_id: [u8; 16],
    /// Product revision (4 bytes)
    pub product_revision: [u8; 4],
}

/// SCSI Read Capacity (10) response
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ScsiReadCapacity10Response {
    /// Last logical block address
    pub last_lba: u32,
    /// Block length in bytes
    pub block_length: u32,
}

/// USB Mass Storage device state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbMscState {
    /// Device not connected
    Disconnected,
    /// Device connected but not configured
    Connected,
    /// Device configured and ready
    Ready,
    /// Error state
    Error,
}

/// USB Mass Storage driver implementation
#[derive(Debug)]
pub struct UsbMassStorageDriver {
    name: String,
    state: StorageDeviceState,
    capabilities: StorageCapabilities,
    stats: StorageStats,
    usb_state: UsbMscState,
    subclass: u8,
    protocol: u8,
    vendor_id: u16,
    product_id: u16,
    lun_count: u8,
    active_lun: u8,
    inquiry_data: Option<ScsiInquiryResponse>,
    block_size: u32,
    block_count: u64,
    tag_counter: u32,
    /// USB host device slot when attached to the xHCI framework.
    host_device_id: Option<u32>,
}

impl UsbMassStorageDriver {
    /// Create new USB Mass Storage driver
    pub fn new(name: String, vendor_id: u16, product_id: u16, subclass: u8, protocol: u8) -> Self {
        Self {
            name,
            state: StorageDeviceState::Offline,
            capabilities: StorageCapabilities::default(),
            stats: StorageStats::default(),
            usb_state: UsbMscState::Disconnected,
            subclass,
            protocol,
            vendor_id,
            product_id,
            lun_count: 1,
            active_lun: 0,
            inquiry_data: None,
            block_size: 512,
            block_count: 0,
            tag_counter: 1,
            host_device_id: None,
        }
    }

    /// Attach to a USB host enumerated device slot.
    pub fn attach_host(&mut self, device_id: u32) {
        self.host_device_id = Some(device_id);
    }

    pub fn host_device_id(&self) -> Option<u32> {
        self.host_device_id
    }

    /// Get next transaction tag
    fn next_tag(&mut self) -> u32 {
        let tag = self.tag_counter;
        self.tag_counter = self.tag_counter.wrapping_add(1);
        tag
    }

    /// Execute SCSI command via Bulk-Only Transport
    fn execute_scsi_command(
        &mut self,
        command: &[u8],
        data_length: u32,
        direction_in: bool,
        buffer: Option<&mut [u8]>,
    ) -> Result<CommandStatusWrapper, StorageError> {
        if self.protocol != UsbMscProtocol::BulkOnly as u8 {
            return Err(StorageError::NotSupported);
        }

        let tag = self.next_tag();
        if let Some(host_id) = self.host_device_id {
            return crate::drivers::usb::msc_execute_scsi(
                host_id,
                command,
                data_length,
                direction_in,
                buffer,
                tag,
            );
        }

        let _ = (command, data_length, direction_in, tag);
        Err(StorageError::NotSupported)
    }

    /// Execute SCSI Inquiry command
    fn scsi_inquiry(&mut self) -> Result<(), StorageError> {
        let command = [ScsiCommand::Inquiry as u8, 0, 0, 0, 36, 0];
        let mut inq_buf = [0u8; 36];
        let csw = self.execute_scsi_command(&command, 36, true, Some(&mut inq_buf))?;
        if !csw.is_success() {
            return Err(StorageError::HardwareError);
        }

        let response =
            unsafe { core::ptr::read_unaligned(inq_buf.as_ptr() as *const ScsiInquiryResponse) };
        self.inquiry_data = Some(response);

        let peripheral_type = response.peripheral & 0x1F;
        if peripheral_type == 0x00 && (response.removable & 0x80) != 0 {
            self.capabilities.is_removable = true;
        } else if peripheral_type == 0x05 {
            self.capabilities.is_removable = true;
        }

        Ok(())
    }

    /// Execute SCSI Read Capacity command
    fn scsi_read_capacity(&mut self) -> Result<(), StorageError> {
        let command = [ScsiCommand::ReadCapacity10 as u8, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut cap_buf = [0u8; 8];
        let csw = self.execute_scsi_command(&command, 8, true, Some(&mut cap_buf))?;
        if !csw.is_success() {
            return Err(StorageError::HardwareError);
        }

        let last_lba = u32::from_be_bytes([cap_buf[0], cap_buf[1], cap_buf[2], cap_buf[3]]);
        self.block_size = u32::from_be_bytes([cap_buf[4], cap_buf[5], cap_buf[6], cap_buf[7]]);
        self.block_count = u64::from(last_lba) + 1;

        self.capabilities.capacity_bytes = self.block_count * self.block_size as u64;
        self.capabilities.sector_size = self.block_size;
        self.capabilities.max_transfer_size = 64 * 1024;

        Ok(())
    }

    /// Execute SCSI Test Unit Ready command
    fn scsi_test_unit_ready(&mut self) -> Result<bool, StorageError> {
        let command = [ScsiCommand::TestUnitReady as u8, 0, 0, 0, 0, 0];
        let csw = self.execute_scsi_command(&command, 0, false, None)?;

        Ok(csw.is_success())
    }

    /// Execute SCSI Read command
    fn scsi_read(
        &mut self,
        lba: u64,
        block_count: u32,
        _buffer: &mut [u8],
    ) -> Result<(), StorageError> {
        if block_count > 65535 {
            return Err(StorageError::TransferTooLarge);
        }

        let data_length = block_count * self.block_size;

        if lba > 0xFFFFFFFF {
            // Use READ(16) for large LBAs
            let command = [
                ScsiCommand::Read16 as u8,
                0, // Flags
                ((lba >> 56) & 0xFF) as u8,
                ((lba >> 48) & 0xFF) as u8,
                ((lba >> 40) & 0xFF) as u8,
                ((lba >> 32) & 0xFF) as u8,
                ((lba >> 24) & 0xFF) as u8,
                ((lba >> 16) & 0xFF) as u8,
                ((lba >> 8) & 0xFF) as u8,
                (lba & 0xFF) as u8,
                ((block_count >> 24) & 0xFF) as u8,
                ((block_count >> 16) & 0xFF) as u8,
                ((block_count >> 8) & 0xFF) as u8,
                (block_count & 0xFF) as u8,
                0, // Group number
                0, // Control
            ];
            self.execute_scsi_command(&command, data_length, true, Some(_buffer))?;
        } else {
            // Use READ(10) for smaller LBAs
            let command = [
                ScsiCommand::Read10 as u8,
                0, // Flags
                ((lba >> 24) & 0xFF) as u8,
                ((lba >> 16) & 0xFF) as u8,
                ((lba >> 8) & 0xFF) as u8,
                (lba & 0xFF) as u8,
                0, // Group number
                ((block_count >> 8) & 0xFF) as u8,
                (block_count & 0xFF) as u8,
                0, // Control
            ];
            self.execute_scsi_command(&command, data_length, true, Some(_buffer))?;
        }

        Ok(())
    }

    /// Execute SCSI Write command
    fn scsi_write(
        &mut self,
        lba: u64,
        block_count: u32,
        _buffer: &[u8],
    ) -> Result<(), StorageError> {
        if block_count > 65535 {
            return Err(StorageError::TransferTooLarge);
        }

        let data_length = block_count * self.block_size;

        if lba > 0xFFFFFFFF {
            // Use WRITE(16) for large LBAs
            let command = [
                ScsiCommand::Write16 as u8,
                0, // Flags
                ((lba >> 56) & 0xFF) as u8,
                ((lba >> 48) & 0xFF) as u8,
                ((lba >> 40) & 0xFF) as u8,
                ((lba >> 32) & 0xFF) as u8,
                ((lba >> 24) & 0xFF) as u8,
                ((lba >> 16) & 0xFF) as u8,
                ((lba >> 8) & 0xFF) as u8,
                (lba & 0xFF) as u8,
                ((block_count >> 24) & 0xFF) as u8,
                ((block_count >> 16) & 0xFF) as u8,
                ((block_count >> 8) & 0xFF) as u8,
                (block_count & 0xFF) as u8,
                0, // Group number
                0, // Control
            ];
            self.execute_scsi_command(&command, data_length, false, None)?;
        } else {
            // Use WRITE(10) for smaller LBAs
            let command = [
                ScsiCommand::Write10 as u8,
                0, // Flags
                ((lba >> 24) & 0xFF) as u8,
                ((lba >> 16) & 0xFF) as u8,
                ((lba >> 8) & 0xFF) as u8,
                (lba & 0xFF) as u8,
                0, // Group number
                ((block_count >> 8) & 0xFF) as u8,
                (block_count & 0xFF) as u8,
                0, // Control
            ];
            self.execute_scsi_command(&command, data_length, false, None)?;
        }

        Ok(())
    }

    /// Get device information
    pub fn get_device_info(&self) -> Option<(String, String, String)> {
        if let Some(ref inquiry) = self.inquiry_data {
            let vendor = String::from_utf8_lossy(&inquiry.vendor_id)
                .trim()
                .to_string();
            let product = String::from_utf8_lossy(&inquiry.product_id)
                .trim()
                .to_string();
            let revision = String::from_utf8_lossy(&inquiry.product_revision)
                .trim()
                .to_string();
            Some((vendor, product, revision))
        } else {
            None
        }
    }

    /// Check if device supports a specific SCSI command
    pub fn supports_command(&self, _command: ScsiCommand) -> bool {
        // In a real implementation, this would check the device's supported commands
        // For now, assume basic commands are supported
        true
    }

    /// Get USB device identifiers
    pub fn get_usb_ids(&self) -> (u16, u16) {
        (self.vendor_id, self.product_id)
    }

    /// Get protocol and subclass
    pub fn get_protocol_info(&self) -> (u8, u8) {
        (self.subclass, self.protocol)
    }
}

impl StorageDriver for UsbMassStorageDriver {
    fn name(&self) -> &str {
        &self.name
    }

    fn device_type(&self) -> StorageDeviceType {
        if self.capabilities.is_removable {
            StorageDeviceType::UsbMassStorage
        } else {
            StorageDeviceType::UsbMassStorage
        }
    }

    fn state(&self) -> StorageDeviceState {
        self.state
    }

    fn capabilities(&self) -> StorageCapabilities {
        self.capabilities.clone()
    }

    fn init(&mut self) -> Result<(), StorageError> {
        self.state = StorageDeviceState::Initializing;

        if self.host_device_id.is_none() {
            return Err(StorageError::NotSupported);
        }

        self.usb_state = UsbMscState::Connected;

        if !self.scsi_test_unit_ready()? {
            return Err(StorageError::HardwareError);
        }

        self.scsi_inquiry()?;
        self.scsi_read_capacity()?;

        self.state = StorageDeviceState::Ready;
        self.usb_state = UsbMscState::Ready;
        Ok(())
    }

    fn read_sectors(
        &mut self,
        start_sector: u64,
        buffer: &mut [u8],
    ) -> Result<usize, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        let sector_size = self.capabilities.sector_size as usize;
        let sector_count = buffer.len() / sector_size;

        if sector_count == 0 {
            return Err(StorageError::BufferTooSmall);
        }

        let lba = start_sector;
        self.scsi_read(lba, sector_count as u32, buffer)?;

        Ok(buffer.len())
    }

    fn write_sectors(&mut self, start_sector: u64, buffer: &[u8]) -> Result<usize, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        let sector_size = self.capabilities.sector_size as usize;
        let sector_count = buffer.len() / sector_size;

        if sector_count == 0 {
            return Err(StorageError::BufferTooSmall);
        }

        let lba = start_sector;
        self.scsi_write(lba, sector_count as u32, buffer)?;

        Ok(buffer.len())
    }

    fn flush(&mut self) -> Result<(), StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        // Execute SYNCHRONIZE CACHE command
        let command = [
            ScsiCommand::SynchronizeCache as u8,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        self.execute_scsi_command(&command, 0, false, None)?;

        Ok(())
    }

    fn get_stats(&self) -> StorageStats {
        self.stats.clone()
    }

    fn reset(&mut self) -> Result<(), StorageError> {
        self.state = StorageDeviceState::Resetting;

        // In a real implementation, we would reset the USB device
        self.usb_state = UsbMscState::Connected;

        // Re-initialize
        self.init()?;
        Ok(())
    }

    fn standby(&mut self) -> Result<(), StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        // Execute START STOP UNIT command to stop
        let command = [ScsiCommand::StartStopUnit as u8, 0, 0, 0, 0, 0];
        self.execute_scsi_command(&command, 0, false, None)?;

        self.state = StorageDeviceState::Standby;
        Ok(())
    }

    fn wake(&mut self) -> Result<(), StorageError> {
        if self.state == StorageDeviceState::Standby {
            // Execute START STOP UNIT command to start
            let command = [ScsiCommand::StartStopUnit as u8, 0, 0, 0, 1, 0];
            self.execute_scsi_command(&command, 0, false, None)?;

            self.state = StorageDeviceState::Ready;
        }
        Ok(())
    }

    fn vendor_command(&mut self, command: u8, data: &[u8]) -> Result<Vec<u8>, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        // Create vendor-specific SCSI command
        let mut cmd = [0u8; 16];
        cmd[0] = command;
        if data.len() > 15 {
            return Err(StorageError::BufferTooSmall);
        }
        cmd[1..1 + data.len()].copy_from_slice(data);

        self.execute_scsi_command(&cmd, 0, false, None)?;

        Ok(Vec::new())
    }

    fn get_smart_data(&mut self) -> Result<Vec<u8>, StorageError> {
        // USB Mass Storage typically doesn't support SMART directly
        Err(StorageError::NotSupported)
    }
}

/// Create USB Mass Storage driver from USB device information
pub fn create_usb_mass_storage_driver(
    vendor_id: u16,
    product_id: u16,
    subclass: u8,
    protocol: u8,
    device_name: Option<String>,
) -> Box<dyn StorageDriver> {
    let name =
        device_name.unwrap_or_else(|| format!("USB MSC {:04x}:{:04x}", vendor_id, product_id));

    let driver = UsbMassStorageDriver::new(name, vendor_id, product_id, subclass, protocol);
    Box::new(driver)
}

/// Create USB Mass Storage driver pre-attached to a host device slot.
pub fn create_usb_mass_storage_driver_with_host(
    host_device_id: u32,
    vendor_id: u16,
    product_id: u16,
    subclass: u8,
    protocol: u8,
    device_name: Option<String>,
) -> Box<dyn StorageDriver> {
    let name =
        device_name.unwrap_or_else(|| format!("USB MSC {:04x}:{:04x}", vendor_id, product_id));

    let mut driver = UsbMassStorageDriver::new(name, vendor_id, product_id, subclass, protocol);
    driver.attach_host(host_device_id);
    Box::new(driver)
}

/// Check if USB device is a Mass Storage device
pub fn is_usb_mass_storage_device(class: u8, subclass: u8, protocol: u8) -> bool {
    class == USB_CLASS_MASS_STORAGE &&
    matches!(subclass, 0x01..=0x08) && // Valid MSC subclasses
    matches!(protocol, 0x00 | 0x01 | 0x50 | 0x62) // Valid MSC protocols
}
