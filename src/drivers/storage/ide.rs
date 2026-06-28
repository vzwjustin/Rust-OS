//! # IDE/PATA Storage Driver
//!
//! Legacy IDE (Integrated Drive Electronics) and PATA (Parallel ATA) driver
//! for older hard drives and optical drives.

use super::{
    StorageCapabilities, StorageDeviceState, StorageDeviceType, StorageDriver, StorageError,
    StorageStats,
};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::arch::asm;

/// IDE register offsets for primary controller
pub const IDE_PRIMARY_IO: u16 = 0x1F0;
pub const IDE_PRIMARY_CTRL: u16 = 0x3F6;

/// IDE register offsets for secondary controller
pub const IDE_SECONDARY_IO: u16 = 0x170;
pub const IDE_SECONDARY_CTRL: u16 = 0x376;

/// IDE I/O port registers (relative to base)
#[repr(u16)]
pub enum IdeIoReg {
    Data = 0,        // Data port
    Features = 1,    // Features/Error information
    SectorCount = 2, // Sector count
    LbaLow = 3,      // LBA bits 0-7
    LbaMid = 4,      // LBA bits 8-15
    LbaHigh = 5,     // LBA bits 16-23
    DriveHead = 6,   // Drive/Head register
    Status = 7,      // Status register (read) / Command register (write)
}

/// IDE control register
#[repr(u16)]
pub enum IdeCtrlReg {
    AltStatus = 0,    // Alternative status (read) / Device control (write)
    DriveAddress = 1, // Drive address
}

// IDE status register bits
bitflags::bitflags! {
    pub struct IdeStatus: u8 {
        const ERR = 1 << 0;     // Error
        const IDX = 1 << 1;     // Index (obsolete)
        const CORR = 1 << 2;    // Corrected data (obsolete)
        const DRQ = 1 << 3;     // Data request
        const DSC = 1 << 4;     // Drive seek complete
        const DF = 1 << 5;      // Drive fault
        const DRDY = 1 << 6;    // Drive ready
        const BSY = 1 << 7;     // Busy
    }
}

// IDE error register bits
bitflags::bitflags! {
    pub struct IdeError: u8 {
        const AMNF = 1 << 0;    // Address mark not found
        const TK0NF = 1 << 1;   // Track 0 not found
        const ABRT = 1 << 2;    // Aborted command
        const MCR = 1 << 3;     // Media change request
        const IDNF = 1 << 4;    // ID not found
        const MC = 1 << 5;      // Media changed
        const UNC = 1 << 6;     // Uncorrectable data error
        const BBK = 1 << 7;     // Bad block detected
    }
}

// IDE device control register bits
bitflags::bitflags! {
    pub struct IdeDevCtrl: u8 {
        const NIEN = 1 << 1;    // Disable interrupts
        const SRST = 1 << 2;    // Software reset
        const HOB = 1 << 7;     // High Order Byte (48-bit LBA)
    }
}

/// IDE commands
#[repr(u8)]
pub enum IdeCommand {
    ReadSectors = 0x20,
    ReadSectorsExt = 0x24,
    WriteSectors = 0x30,
    WriteSectorsExt = 0x34,
    IdentifyDevice = 0xEC,
    IdentifyPacketDevice = 0xA1,
    SetFeatures = 0xEF,
    FlushCache = 0xE7,
    FlushCacheExt = 0xEA,
    StandbyImmediate = 0xE0,
    IdleImmediate = 0xE1,
    Packet = 0xA0,
    DeviceReset = 0x08,
}

/// IDE device type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdeDeviceType {
    /// ATA hard drive
    Ata,
    /// ATAPI device (CD/DVD)
    Atapi,
    /// No device
    None,
}

/// IDE identify device data structure (512 bytes, partial)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IdeIdentify {
    /// General configuration
    pub general_config: u16,
    /// Number of logical cylinders
    pub cylinders: u16,
    /// Specific configuration
    pub specific_config: u16,
    /// Number of logical heads
    pub heads: u16,
    /// Retired
    pub retired1: [u16; 2],
    /// Number of logical sectors per track
    pub sectors_per_track: u16,
    /// Retired
    pub retired2: [u16; 3],
    /// Serial number (20 ASCII characters)
    pub serial_number: [u16; 10],
    /// Retired
    pub retired3: [u16; 3],
    /// Firmware revision (8 ASCII characters)
    pub firmware_revision: [u16; 4],
    /// Model number (40 ASCII characters)
    pub model_number: [u16; 20],
    /// Maximum number of sectors per interrupt
    pub max_sectors_per_int: u16,
    /// Trusted computing feature set options
    pub trusted_computing: u16,
    /// Capabilities
    pub capabilities: [u16; 2],
    /// Obsolete
    pub obsolete: [u16; 2],
    /// Fields valid
    pub fields_valid: u16,
    /// Current logical cylinders
    pub current_cylinders: u16,
    /// Current logical heads
    pub current_heads: u16,
    /// Current logical sectors per track
    pub current_sectors_per_track: u16,
    /// Current capacity in sectors
    pub current_capacity: [u16; 2],
    /// Multiple sector setting
    pub multiple_sector_setting: u16,
    /// Total number of user addressable sectors (28-bit)
    pub user_addressable_sectors: [u16; 2],
    /// Obsolete
    pub obsolete2: u16,
    /// Multiword DMA modes
    pub multiword_dma: u16,
    /// Advanced PIO modes
    pub advanced_pio: u16,
    /// Minimum multiword DMA transfer cycle time
    pub min_multiword_dma_time: u16,
    /// Recommended multiword DMA transfer cycle time
    pub rec_multiword_dma_time: u16,
    /// Minimum PIO transfer cycle time without IORDY
    pub min_pio_time: u16,
    /// Minimum PIO transfer cycle time with IORDY
    pub min_pio_time_iordy: u16,
    /// Additional supported
    pub additional_supported: [u16; 2],
    /// Reserved
    pub reserved: [u16; 4],
    /// Queue depth
    pub queue_depth: u16,
    /// Serial ATA capabilities
    pub sata_capabilities: u16,
    /// Serial ATA additional capabilities
    pub sata_additional_capabilities: u16,
    /// Serial ATA features supported
    pub sata_features_supported: u16,
    /// Serial ATA features enabled
    pub sata_features_enabled: u16,
    /// Major version number
    pub major_version: u16,
    /// Minor version number
    pub minor_version: u16,
    /// Command set supported
    pub command_set_supported: [u16; 3],
    /// Command set enabled
    pub command_set_enabled: [u16; 3],
    /// Ultra DMA modes
    pub ultra_dma: u16,
    /// Security erase time
    pub security_erase_time: [u16; 2],
    /// Enhanced security erase time
    pub enhanced_security_erase_time: [u16; 2],
    /// Current advanced power management value
    pub current_apm: u16,
    /// Master password revision
    pub master_password_revision: u16,
    /// Hardware configuration test results
    pub hw_config_test: u16,
    /// Acoustic management
    pub acoustic_management: u16,
    /// Stream minimum request size
    pub stream_min_request_size: u16,
    /// Streaming transfer time DMA
    pub streaming_transfer_time_dma: u16,
    /// Streaming access latency
    pub streaming_access_latency: u16,
    /// Streaming performance granularity
    pub streaming_performance_granularity: [u16; 2],
    /// Maximum user LBA for 48-bit addressing
    pub max_lba_48: [u16; 4],
    /// Streaming transfer time PIO
    pub streaming_transfer_time_pio: u16,
    /// Maximum sectors per DATA SET MANAGEMENT command
    pub max_sectors_per_dsm: u16,
    /// Physical sector size / logical sector size
    pub physical_logical_sector_size: u16,
    /// Inter-seek delay for acoustic testing
    pub inter_seek_delay: u16,
    /// World wide name
    pub world_wide_name: [u16; 4],
    /// Reserved for world wide name extension
    pub wwn_extension: [u16; 4],
    /// Reserved for technical report
    pub reserved_technical_report: u16,
    /// Logical sector size
    pub logical_sector_size: [u16; 2],
    /// Commands and feature sets supported (Continued)
    pub commands_feature_sets_supported: [u16; 2],
    /// Reserved
    pub reserved2: [u16; 6],
    /// Alignment of logical sectors
    pub alignment_logical_sectors: u16,
    /// Write-Read-Verify sector count mode 3
    pub wrv_sector_count_mode3: [u16; 2],
    /// Write-Read-Verify sector count mode 2
    pub wrv_sector_count_mode2: [u16; 2],
    /// NV Cache capabilities
    pub nv_cache_capabilities: u16,
    /// NV Cache size
    pub nv_cache_size: [u16; 2],
    /// Nominal media rotation rate
    pub nominal_media_rotation_rate: u16,
    /// Reserved
    pub reserved3: u16,
    /// NV Cache options
    pub nv_cache_options: u16,
    /// Write-Read-Verify feature set current mode
    pub wrv_current_mode: u16,
    /// Reserved
    pub reserved4: u16,
    /// Transport major version number
    pub transport_major_version: u16,
    /// Transport minor version number
    pub transport_minor_version: u16,
    /// Reserved
    pub reserved5: [u16; 6],
    /// Extended number of user addressable sectors
    pub extended_user_addressable_sectors: [u16; 4],
    /// Minimum number of 512-byte units per DOWNLOAD MICROCODE command
    pub min_download_microcode_units: u16,
    /// Maximum number of 512-byte units per DOWNLOAD MICROCODE command
    pub max_download_microcode_units: u16,
    /// Reserved
    pub reserved6: [u16; 19],
    /// Integrity word
    pub integrity_word: u16,
}

/// IDE driver implementation
#[derive(Debug)]
pub struct IdeDriver {
    name: String,
    state: StorageDeviceState,
    capabilities: StorageCapabilities,
    stats: StorageStats,
    io_base: u16,
    ctrl_base: u16,
    drive: u8, // 0 = master, 1 = slave
    device_type: IdeDeviceType,
    supports_lba28: bool,
    supports_lba48: bool,
    identify_data: Option<IdeIdentify>,
}

impl IdeDriver {
    /// Create new IDE driver instance
    pub fn new(name: String, is_secondary: bool, is_slave: bool) -> Self {
        let (io_base, ctrl_base) = if is_secondary {
            (IDE_SECONDARY_IO, IDE_SECONDARY_CTRL)
        } else {
            (IDE_PRIMARY_IO, IDE_PRIMARY_CTRL)
        };

        Self {
            name,
            state: StorageDeviceState::Offline,
            capabilities: StorageCapabilities::default(),
            stats: StorageStats::default(),
            io_base,
            ctrl_base,
            drive: if is_slave { 1 } else { 0 },
            device_type: IdeDeviceType::None,
            supports_lba28: false,
            supports_lba48: false,
            identify_data: None,
        }
    }

    /// Read IDE I/O register
    fn read_io_reg(&self, reg: IdeIoReg) -> u8 {
        let port = self.io_base + reg as u16;
        unsafe {
            let mut value: u8;
            asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
            value
        }
    }

    /// Write IDE I/O register
    fn write_io_reg(&self, reg: IdeIoReg, value: u8) {
        let port = self.io_base + reg as u16;
        unsafe {
            asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
        }
    }

    /// Read IDE control register
    fn read_ctrl_reg(&self, reg: IdeCtrlReg) -> u8 {
        let port = self.ctrl_base + reg as u16;
        unsafe {
            let mut value: u8;
            asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
            value
        }
    }

    /// Write IDE control register
    fn write_ctrl_reg(&self, reg: IdeCtrlReg, value: u8) {
        let port = self.ctrl_base + reg as u16;
        unsafe {
            asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
        }
    }

    /// Read data port (16-bit)
    fn read_data(&self) -> u16 {
        let port = self.io_base + IdeIoReg::Data as u16;
        unsafe {
            let mut value: u16;
            asm!("in ax, dx", out("ax") value, in("dx") port, options(nomem, nostack, preserves_flags));
            value
        }
    }

    /// Write data port (16-bit)
    fn write_data(&self, value: u16) {
        let port = self.io_base + IdeIoReg::Data as u16;
        unsafe {
            asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
        }
    }

    /// Wait for device to be ready
    fn wait_ready(&self) -> Result<(), StorageError> {
        for _ in 0..50000 {
            let status = self.read_io_reg(IdeIoReg::Status);
            if (status & IdeStatus::BSY.bits()) == 0 {
                if (status & IdeStatus::DRDY.bits()) != 0 {
                    return Ok(());
                }
            }
        }
        Err(StorageError::Timeout)
    }

    /// Wait for data request
    fn wait_drq(&self) -> Result<(), StorageError> {
        for _ in 0..50000 {
            let status = self.read_io_reg(IdeIoReg::Status);
            if (status & IdeStatus::BSY.bits()) == 0 {
                if (status & IdeStatus::DRQ.bits()) != 0 {
                    return Ok(());
                }
                if (status & IdeStatus::ERR.bits()) != 0 {
                    return Err(StorageError::HardwareError);
                }
            }
        }
        Err(StorageError::Timeout)
    }

    /// Select drive
    fn select_drive(&self) -> Result<(), StorageError> {
        // Drive/Head register: 0xA0 for drive 0, 0xB0 for drive 1
        let drive_select = 0xA0 | (self.drive << 4);
        self.write_io_reg(IdeIoReg::DriveHead, drive_select);

        // Small delay
        for _ in 0..400 {
            self.read_ctrl_reg(IdeCtrlReg::AltStatus);
        }

        self.wait_ready()
    }

    /// Reset IDE controller
    fn reset_controller(&self) -> Result<(), StorageError> {
        // Set reset bit
        self.write_ctrl_reg(IdeCtrlReg::AltStatus, IdeDevCtrl::SRST.bits());

        // Wait
        for _ in 0..1000 {
            self.read_ctrl_reg(IdeCtrlReg::AltStatus);
        }

        // Clear reset bit
        self.write_ctrl_reg(IdeCtrlReg::AltStatus, 0);

        // Wait for ready
        for _ in 0..50000 {
            let status = self.read_ctrl_reg(IdeCtrlReg::AltStatus);
            if (status & IdeStatus::BSY.bits()) == 0 {
                break;
            }
        }

        Ok(())
    }

    /// Identify device
    fn identify_device(&mut self) -> Result<(), StorageError> {
        self.select_drive()?;

        // Clear sector count, LBA registers
        self.write_io_reg(IdeIoReg::SectorCount, 0);
        self.write_io_reg(IdeIoReg::LbaLow, 0);
        self.write_io_reg(IdeIoReg::LbaMid, 0);
        self.write_io_reg(IdeIoReg::LbaHigh, 0);

        // Send IDENTIFY command
        self.write_io_reg(IdeIoReg::Status, IdeCommand::IdentifyDevice as u8);

        // Check if drive exists
        let status = self.read_io_reg(IdeIoReg::Status);
        if status == 0 {
            return Err(StorageError::DeviceNotFound);
        }

        // Wait for response
        self.wait_ready()?;

        // Check LBA registers to determine device type
        let lba_mid = self.read_io_reg(IdeIoReg::LbaMid);
        let lba_high = self.read_io_reg(IdeIoReg::LbaHigh);

        match (lba_mid, lba_high) {
            (0x00, 0x00) => {
                // ATA device
                self.device_type = IdeDeviceType::Ata;
                self.wait_drq()?;
                self.read_identify_data()?;
            }
            (0x14, 0xEB) => {
                // ATAPI device
                self.device_type = IdeDeviceType::Atapi;
                // Send IDENTIFY PACKET DEVICE command
                self.write_io_reg(IdeIoReg::Status, IdeCommand::IdentifyPacketDevice as u8);
                self.wait_drq()?;
                self.read_identify_data()?;
            }
            _ => {
                return Err(StorageError::DeviceNotFound);
            }
        }

        Ok(())
    }

    /// Read identify data
    fn read_identify_data(&mut self) -> Result<(), StorageError> {
        let mut data = [0u16; 256];

        // Read 256 words (512 bytes)
        for i in 0..256 {
            data[i] = self.read_data();
        }

        // Parse identify data
        let identify = unsafe { *(data.as_ptr() as *const IdeIdentify) };
        self.identify_data = Some(identify);

        // Extract capabilities
        if self.device_type == IdeDeviceType::Ata {
            // Check LBA support
            if (identify.capabilities[0] & (1 << 9)) != 0 {
                self.supports_lba28 = true;
            }

            // Check 48-bit LBA support
            if (identify.command_set_supported[1] & (1 << 10)) != 0 {
                self.supports_lba48 = true;
            }

            // Get capacity
            if self.supports_lba48 {
                let sectors = u64::from_le_bytes([
                    identify.max_lba_48[0] as u8,
                    (identify.max_lba_48[0] >> 8) as u8,
                    identify.max_lba_48[1] as u8,
                    (identify.max_lba_48[1] >> 8) as u8,
                    identify.max_lba_48[2] as u8,
                    (identify.max_lba_48[2] >> 8) as u8,
                    identify.max_lba_48[3] as u8,
                    (identify.max_lba_48[3] >> 8) as u8,
                ]);
                self.capabilities.capacity_bytes = sectors * 512;
            } else if self.supports_lba28 {
                let sectors = u32::from_le_bytes([
                    identify.user_addressable_sectors[0] as u8,
                    (identify.user_addressable_sectors[0] >> 8) as u8,
                    identify.user_addressable_sectors[1] as u8,
                    (identify.user_addressable_sectors[1] >> 8) as u8,
                ]);
                self.capabilities.capacity_bytes = sectors as u64 * 512;
            } else {
                // CHS mode
                let sectors = identify.cylinders as u64
                    * identify.heads as u64
                    * identify.sectors_per_track as u64;
                self.capabilities.capacity_bytes = sectors * 512;
            }

            // Set typical HDD speeds
            self.capabilities.read_speed_mbps = 150;
            self.capabilities.write_speed_mbps = 150;
        } else {
            // ATAPI device (CD/DVD)
            self.capabilities.capacity_bytes = 700 * 1024 * 1024; // Assume CD capacity
            self.capabilities.is_removable = true;
            self.capabilities.read_speed_mbps = 24; // 24x CD speed
            self.capabilities.write_speed_mbps = 16;
        }

        // Check for SMART support
        if (identify.command_set_supported[0] & (1 << 0)) != 0 {
            self.capabilities.supports_smart = true;
        }

        self.capabilities.sector_size = 512;
        self.capabilities.max_transfer_size = 64 * 1024; // 64KB typical
        self.capabilities.max_queue_depth = 1; // IDE doesn't support queuing

        Ok(())
    }

    /// Execute LBA28 read/write
    fn execute_lba28(
        &mut self,
        command: IdeCommand,
        lba: u32,
        sector_count: u8,
    ) -> Result<(), StorageError> {
        if !self.supports_lba28 {
            return Err(StorageError::NotSupported);
        }

        self.select_drive()?;

        // Set up registers for LBA28
        self.write_io_reg(IdeIoReg::Features, 0);
        self.write_io_reg(IdeIoReg::SectorCount, sector_count);
        self.write_io_reg(IdeIoReg::LbaLow, (lba & 0xFF) as u8);
        self.write_io_reg(IdeIoReg::LbaMid, ((lba >> 8) & 0xFF) as u8);
        self.write_io_reg(IdeIoReg::LbaHigh, ((lba >> 16) & 0xFF) as u8);

        // Drive/Head register with LBA bit set and upper 4 bits of LBA
        let drive_head = 0xE0 | (self.drive << 4) | (((lba >> 24) & 0x0F) as u8);
        self.write_io_reg(IdeIoReg::DriveHead, drive_head);

        // Send command
        self.write_io_reg(IdeIoReg::Status, command as u8);

        Ok(())
    }

    /// Execute LBA48 read/write
    fn execute_lba48(
        &mut self,
        command: IdeCommand,
        lba: u64,
        sector_count: u16,
    ) -> Result<(), StorageError> {
        if !self.supports_lba48 {
            return Err(StorageError::NotSupported);
        }

        self.select_drive()?;

        // Set up registers for LBA48 (HOB first)
        self.write_io_reg(IdeIoReg::SectorCount, (sector_count >> 8) as u8);
        self.write_io_reg(IdeIoReg::LbaLow, ((lba >> 24) & 0xFF) as u8);
        self.write_io_reg(IdeIoReg::LbaMid, ((lba >> 32) & 0xFF) as u8);
        self.write_io_reg(IdeIoReg::LbaHigh, ((lba >> 40) & 0xFF) as u8);

        // Low bytes
        self.write_io_reg(IdeIoReg::SectorCount, (sector_count & 0xFF) as u8);
        self.write_io_reg(IdeIoReg::LbaLow, (lba & 0xFF) as u8);
        self.write_io_reg(IdeIoReg::LbaMid, ((lba >> 8) & 0xFF) as u8);
        self.write_io_reg(IdeIoReg::LbaHigh, ((lba >> 16) & 0xFF) as u8);

        // Drive register with LBA bit set
        let drive_head = 0x40 | (self.drive << 4);
        self.write_io_reg(IdeIoReg::DriveHead, drive_head);

        // Send command
        self.write_io_reg(IdeIoReg::Status, command as u8);

        Ok(())
    }

    /// Read sectors from buffer
    fn read_sector_data(&self, buffer: &mut [u8]) -> Result<(), StorageError> {
        if buffer.len() != 512 {
            return Err(StorageError::BufferTooSmall);
        }

        self.wait_drq()?;

        // Read 256 words (512 bytes)
        let words =
            unsafe { core::slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u16, 256) };
        for word in words.iter_mut() {
            *word = self.read_data();
        }

        Ok(())
    }

    /// Write sectors to buffer
    fn write_sector_data(&self, buffer: &[u8]) -> Result<(), StorageError> {
        if buffer.len() != 512 {
            return Err(StorageError::BufferTooSmall);
        }

        self.wait_drq()?;

        // Write 256 words (512 bytes)
        let words = unsafe { core::slice::from_raw_parts(buffer.as_ptr() as *const u16, 256) };
        for &word in words {
            self.write_data(word);
        }

        Ok(())
    }

    /// Get device model string from IDENTIFY data.
    fn model_string(&self) -> Option<String> {
        if let Some(ref identify) = self.identify_data {
            // SAFETY: identify is a packed struct representing IDE on-disk format.
            // We use addr_of! to avoid creating misaligned references.
            let model_bytes: &[u8] = unsafe {
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(identify.model_number) as *const u8,
                    40,
                )
            };

            // Convert from UTF-16BE-like to ASCII (IDE uses byte-swapped words)
            let mut model = Vec::new();
            for i in (0..40).step_by(2) {
                if i + 1 < model_bytes.len() {
                    model.push(model_bytes[i + 1]);
                    model.push(model_bytes[i]);
                }
            }

            // Trim trailing spaces and convert to string
            while model.last() == Some(&b' ') {
                model.pop();
            }

            String::from_utf8(model).ok()
        } else {
            None
        }
    }

    /// Get device serial number from IDENTIFY data.
    fn serial_string(&self) -> Option<String> {
        if let Some(ref identify) = self.identify_data {
            // SAFETY: identify is a packed struct representing IDE on-disk format.
            // We use addr_of! to avoid creating misaligned references.
            let serial_bytes: &[u8] = unsafe {
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(identify.serial_number) as *const u8,
                    20,
                )
            };

            // Convert byte-swapped words to ASCII
            let mut serial = Vec::new();
            for i in (0..20).step_by(2) {
                if i + 1 < serial_bytes.len() {
                    serial.push(serial_bytes[i + 1]);
                    serial.push(serial_bytes[i]);
                }
            }

            // Trim trailing spaces
            while serial.last() == Some(&b' ') {
                serial.pop();
            }

            String::from_utf8(serial).ok()
        } else {
            None
        }
    }
}

impl StorageDriver for IdeDriver {
    fn name(&self) -> &str {
        &self.name
    }

    fn device_type(&self) -> StorageDeviceType {
        match self.device_type {
            IdeDeviceType::Ata => StorageDeviceType::IdeDrive,
            IdeDeviceType::Atapi => StorageDeviceType::OpticalDrive,
            IdeDeviceType::None => StorageDeviceType::Unknown,
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

        // Reset controller
        self.reset_controller()?;

        // Identify device
        self.identify_device()?;

        if self.device_type == IdeDeviceType::None {
            self.state = StorageDeviceState::Offline;
            return Err(StorageError::DeviceNotFound);
        }

        self.state = StorageDeviceState::Ready;
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

        if self.device_type != IdeDeviceType::Ata {
            return Err(StorageError::NotSupported);
        }

        let sector_count = buffer.len() / 512;
        if sector_count == 0 || sector_count > 256 {
            return Err(StorageError::InvalidSector);
        }

        let mut bytes_read = 0;

        for i in 0..sector_count {
            let lba = start_sector + i as u64;
            let sector_buffer = &mut buffer[i * 512..(i + 1) * 512];

            // Choose appropriate command based on capabilities
            if self.supports_lba48 && lba > 0x0FFFFFFF {
                self.execute_lba48(IdeCommand::ReadSectorsExt, lba, 1)?;
            } else if self.supports_lba28 {
                self.execute_lba28(IdeCommand::ReadSectors, lba as u32, 1)?;
            } else {
                return Err(StorageError::NotSupported);
            }

            self.read_sector_data(sector_buffer)?;
            bytes_read += 512;
        }

        self.stats.reads_total += sector_count as u64;
        self.stats.bytes_read += bytes_read as u64;

        Ok(bytes_read)
    }

    fn write_sectors(&mut self, start_sector: u64, buffer: &[u8]) -> Result<usize, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        if self.device_type != IdeDeviceType::Ata {
            return Err(StorageError::NotSupported);
        }

        let sector_count = buffer.len() / 512;
        if sector_count == 0 || sector_count > 256 {
            return Err(StorageError::InvalidSector);
        }

        let mut bytes_written = 0;

        for i in 0..sector_count {
            let lba = start_sector + i as u64;
            let sector_buffer = &buffer[i * 512..(i + 1) * 512];

            // Choose appropriate command based on capabilities
            if self.supports_lba48 && lba > 0x0FFFFFFF {
                self.execute_lba48(IdeCommand::WriteSectorsExt, lba, 1)?;
            } else if self.supports_lba28 {
                self.execute_lba28(IdeCommand::WriteSectors, lba as u32, 1)?;
            } else {
                return Err(StorageError::NotSupported);
            }

            self.write_sector_data(sector_buffer)?;
            bytes_written += 512;
        }

        self.stats.writes_total += sector_count as u64;
        self.stats.bytes_written += bytes_written as u64;

        Ok(bytes_written)
    }

    fn flush(&mut self) -> Result<(), StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        self.select_drive()?;

        if self.supports_lba48 {
            self.write_io_reg(IdeIoReg::Status, IdeCommand::FlushCacheExt as u8);
        } else {
            self.write_io_reg(IdeIoReg::Status, IdeCommand::FlushCache as u8);
        }

        self.wait_ready()?;
        Ok(())
    }

    fn get_stats(&self) -> StorageStats {
        self.stats.clone()
    }

    fn reset(&mut self) -> Result<(), StorageError> {
        self.state = StorageDeviceState::Resetting;
        self.reset_controller()?;
        self.init()?;
        Ok(())
    }

    fn standby(&mut self) -> Result<(), StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        self.select_drive()?;
        self.write_io_reg(IdeIoReg::Status, IdeCommand::StandbyImmediate as u8);
        self.wait_ready()?;
        self.state = StorageDeviceState::Standby;
        Ok(())
    }

    fn wake(&mut self) -> Result<(), StorageError> {
        if self.state == StorageDeviceState::Standby {
            self.select_drive()?;
            self.write_io_reg(IdeIoReg::Status, IdeCommand::IdleImmediate as u8);
            self.wait_ready()?;
            self.state = StorageDeviceState::Ready;
        }
        Ok(())
    }

    fn vendor_command(&mut self, command: u8, _data: &[u8]) -> Result<Vec<u8>, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        self.select_drive()?;
        self.write_io_reg(IdeIoReg::Status, command);
        self.wait_ready()?;

        // Return empty response
        Ok(Vec::new())
    }

    fn get_smart_data(&mut self) -> Result<Vec<u8>, StorageError> {
        if !self.capabilities.supports_smart {
            return Err(StorageError::NotSupported);
        }

        self.select_drive()?;

        // Set up SMART READ DATA registers
        // Features = 0xD0 (SMART READ DATA)
        // LBA low = 0xC0, LBA mid = 0x4F, LBA high = 0xC2 (SMART signature)
        // Sector count = 1
        self.write_io_reg(IdeIoReg::Features, 0xD0);
        self.write_io_reg(IdeIoReg::SectorCount, 1);
        self.write_io_reg(IdeIoReg::LbaLow, 0xC0);
        self.write_io_reg(IdeIoReg::LbaMid, 0x4F);
        self.write_io_reg(IdeIoReg::LbaHigh, 0xC2);

        // Drive/Head register with LBA bit set
        let drive_head = 0xE0 | (self.drive << 4);
        self.write_io_reg(IdeIoReg::DriveHead, drive_head);

        // Send SMART command (0xB0)
        self.write_io_reg(IdeIoReg::Status, 0xB0);

        // Wait for data request
        self.wait_drq()?;

        // Read 512 bytes of SMART data via PIO (256 16-bit reads)
        let mut smart_data = vec![0u8; 512];
        for i in 0..256 {
            let word = self.read_data();
            smart_data[i * 2] = (word & 0xFF) as u8;
            smart_data[i * 2 + 1] = ((word >> 8) & 0xFF) as u8;
        }

        // Wait for command completion
        self.wait_ready()?;

        Ok(smart_data)
    }

    fn get_model(&self) -> Option<String> {
        self.model_string()
    }

    fn get_serial(&self) -> Option<String> {
        self.serial_string()
    }
}

/// Create IDE driver instances for all possible drives
pub fn create_ide_drivers() -> Vec<Box<dyn StorageDriver>> {
    let mut drivers = Vec::new();

    // Primary IDE controller
    drivers.push(Box::new(IdeDriver::new(
        "IDE Primary Master".to_string(),
        false,
        false,
    )) as Box<dyn StorageDriver>);
    drivers.push(
        Box::new(IdeDriver::new("IDE Primary Slave".to_string(), false, true))
            as Box<dyn StorageDriver>,
    );

    // Secondary IDE controller
    drivers.push(Box::new(IdeDriver::new(
        "IDE Secondary Master".to_string(),
        true,
        false,
    )) as Box<dyn StorageDriver>);
    drivers.push(Box::new(IdeDriver::new(
        "IDE Secondary Slave".to_string(),
        true,
        true,
    )) as Box<dyn StorageDriver>);

    drivers
}
