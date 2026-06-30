//! # Storage Drivers Module
//!
//! This module provides comprehensive storage device drivers for RustOS,
//! including AHCI SATA, NVMe, IDE/PATA, and USB Mass Storage support.

pub mod ahci;
pub mod detection;
pub mod filesystem_interface;
pub mod ide;
pub mod nvme;
pub mod pci_scan;
pub mod usb_mass_storage;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use spin::RwLock;

/// Storage device types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageDeviceType {
    /// SATA Hard Drive
    SataHdd,
    /// SATA Solid State Drive
    SataSsd,
    /// NVMe SSD
    NvmeSsd,
    /// IDE/PATA Drive
    IdeDrive,
    /// USB Mass Storage
    UsbMassStorage,
    /// RAID Array
    RaidArray,
    /// Optical Drive
    OpticalDrive,
    /// Unknown storage device
    Unknown,
}

impl fmt::Display for StorageDeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageDeviceType::SataHdd => write!(f, "SATA HDD"),
            StorageDeviceType::SataSsd => write!(f, "SATA SSD"),
            StorageDeviceType::NvmeSsd => write!(f, "NVMe SSD"),
            StorageDeviceType::IdeDrive => write!(f, "IDE Drive"),
            StorageDeviceType::UsbMassStorage => write!(f, "USB Mass Storage"),
            StorageDeviceType::RaidArray => write!(f, "RAID Array"),
            StorageDeviceType::OpticalDrive => write!(f, "Optical Drive"),
            StorageDeviceType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Storage device state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageDeviceState {
    /// Device is offline/not detected
    Offline,
    /// Device is initializing
    Initializing,
    /// Device is ready for I/O
    Ready,
    /// Device is in sleep/standby mode
    Standby,
    /// Device has encountered an error
    Error,
    /// Device is being reset
    Resetting,
}

impl fmt::Display for StorageDeviceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageDeviceState::Offline => write!(f, "OFFLINE"),
            StorageDeviceState::Initializing => write!(f, "INITIALIZING"),
            StorageDeviceState::Ready => write!(f, "READY"),
            StorageDeviceState::Standby => write!(f, "STANDBY"),
            StorageDeviceState::Error => write!(f, "ERROR"),
            StorageDeviceState::Resetting => write!(f, "RESETTING"),
        }
    }
}

/// Storage device capabilities
#[derive(Debug, Clone)]
pub struct StorageCapabilities {
    /// Device capacity in bytes
    pub capacity_bytes: u64,
    /// Sector size in bytes
    pub sector_size: u32,
    /// Maximum transfer size in bytes
    pub max_transfer_size: u32,
    /// Supports TRIM/UNMAP commands
    pub supports_trim: bool,
    /// Supports NCQ (Native Command Queuing)
    pub supports_ncq: bool,
    /// Supports SMART monitoring
    pub supports_smart: bool,
    /// Maximum queue depth
    pub max_queue_depth: u16,
    /// Read speed in MB/s (estimated)
    pub read_speed_mbps: u32,
    /// Write speed in MB/s (estimated)
    pub write_speed_mbps: u32,
    /// Device is removable
    pub is_removable: bool,
}

impl Default for StorageCapabilities {
    fn default() -> Self {
        Self {
            capacity_bytes: 0,
            sector_size: 512,
            max_transfer_size: 65536,
            supports_trim: false,
            supports_ncq: false,
            supports_smart: false,
            max_queue_depth: 1,
            read_speed_mbps: 100,
            write_speed_mbps: 100,
            is_removable: false,
        }
    }
}

/// Storage device error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    /// Device not found
    DeviceNotFound,
    /// Invalid sector address
    InvalidSector,
    /// Transfer too large
    TransferTooLarge,
    /// Device timeout
    Timeout,
    /// Hardware error
    HardwareError,
    /// Media error
    MediaError,
    /// Device busy
    DeviceBusy,
    /// Permission denied
    PermissionDenied,
    /// Not supported
    NotSupported,
    /// Buffer too small
    BufferTooSmall,
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::DeviceNotFound => write!(f, "Device not found"),
            StorageError::InvalidSector => write!(f, "Invalid sector address"),
            StorageError::TransferTooLarge => write!(f, "Transfer too large"),
            StorageError::Timeout => write!(f, "Device timeout"),
            StorageError::HardwareError => write!(f, "Hardware error"),
            StorageError::MediaError => write!(f, "Media error"),
            StorageError::DeviceBusy => write!(f, "Device busy"),
            StorageError::PermissionDenied => write!(f, "Permission denied"),
            StorageError::NotSupported => write!(f, "Not supported"),
            StorageError::BufferTooSmall => write!(f, "Buffer too small"),
        }
    }
}

/// Storage device statistics
#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    /// Total read operations
    pub reads_total: u64,
    /// Total write operations
    pub writes_total: u64,
    /// Total bytes read
    pub bytes_read: u64,
    /// Total bytes written
    pub bytes_written: u64,
    /// Read errors
    pub read_errors: u64,
    /// Write errors
    pub write_errors: u64,
    /// Average read latency in microseconds
    pub avg_read_latency_us: u32,
    /// Average write latency in microseconds
    pub avg_write_latency_us: u32,
    /// Device uptime in seconds
    pub uptime_seconds: u64,
}

/// Storage driver interface
pub trait StorageDriver: Send + Sync + core::fmt::Debug {
    /// Get driver name
    fn name(&self) -> &str;

    /// Get device type
    fn device_type(&self) -> StorageDeviceType;

    /// Get device state
    fn state(&self) -> StorageDeviceState;

    /// Get device capabilities
    fn capabilities(&self) -> StorageCapabilities;

    /// Initialize the device
    fn init(&mut self) -> Result<(), StorageError>;

    /// Read sectors from the device
    fn read_sectors(&mut self, start_sector: u64, buffer: &mut [u8])
        -> Result<usize, StorageError>;

    /// Write sectors to the device
    fn write_sectors(&mut self, start_sector: u64, buffer: &[u8]) -> Result<usize, StorageError>;

    /// Flush any pending writes
    fn flush(&mut self) -> Result<(), StorageError>;

    /// Get device statistics
    fn get_stats(&self) -> StorageStats;

    /// Reset the device
    fn reset(&mut self) -> Result<(), StorageError>;

    /// Put device in standby mode
    fn standby(&mut self) -> Result<(), StorageError>;

    /// Wake device from standby
    fn wake(&mut self) -> Result<(), StorageError>;

    /// Execute vendor-specific command
    fn vendor_command(&mut self, command: u8, data: &[u8]) -> Result<Vec<u8>, StorageError>;

    /// Get SMART data (if supported)
    fn get_smart_data(&mut self) -> Result<Vec<u8>, StorageError>;

    /// Get device model string
    fn get_model(&self) -> Option<String> {
        None
    }

    /// Get device serial number
    fn get_serial(&self) -> Option<String> {
        None
    }
}

/// Storage device descriptor
#[derive(Debug)]
pub struct StorageDevice {
    /// Device ID
    pub id: u32,
    /// Device driver
    pub driver: Box<dyn StorageDriver>,
    /// Device model
    pub model: String,
    /// Device serial number
    pub serial: String,
    /// Firmware version
    pub firmware: String,
    /// Registration timestamp
    pub registered_at: u64,
    /// Last access timestamp
    pub last_access: u64,
}

impl StorageDevice {
    pub fn new(
        id: u32,
        driver: Box<dyn StorageDriver>,
        model: String,
        serial: String,
        firmware: String,
        timestamp: u64,
    ) -> Self {
        Self {
            id,
            driver,
            model,
            serial,
            firmware,
            registered_at: timestamp,
            last_access: timestamp,
        }
    }

    /// Update last access time
    pub fn update_access(&mut self, timestamp: u64) {
        self.last_access = timestamp;
    }

    /// Get device information
    pub fn get_info(&self) -> StorageDeviceInfo {
        StorageDeviceInfo {
            id: self.id,
            name: self.driver.name().into(),
            device_type: self.driver.device_type(),
            state: self.driver.state(),
            capabilities: self.driver.capabilities(),
            stats: self.driver.get_stats(),
            model: self.model.clone(),
            serial: self.serial.clone(),
            firmware: self.firmware.clone(),
            registered_at: self.registered_at,
            last_access: self.last_access,
        }
    }
}

/// Device information structure
#[derive(Debug, Clone)]
pub struct StorageDeviceInfo {
    pub id: u32,
    pub name: String,
    pub device_type: StorageDeviceType,
    pub state: StorageDeviceState,
    pub capabilities: StorageCapabilities,
    pub stats: StorageStats,
    pub model: String,
    pub serial: String,
    pub firmware: String,
    pub registered_at: u64,
    pub last_access: u64,
}

/// Storage driver manager
#[derive(Debug)]
pub struct StorageDriverManager {
    /// Registered devices
    devices: BTreeMap<u32, StorageDevice>,
    /// Next device ID
    next_id: u32,
    /// Manager statistics
    stats: StorageManagerStats,
}

impl StorageDriverManager {
    pub fn new() -> Self {
        Self {
            devices: BTreeMap::new(),
            next_id: 1,
            stats: StorageManagerStats::default(),
        }
    }

    /// Register a storage device
    pub fn register_device(
        &mut self,
        driver: Box<dyn StorageDriver>,
        model: String,
        serial: String,
        firmware: String,
        timestamp: u64,
    ) -> Result<u32, StorageError> {
        let id = self.next_id;
        self.next_id += 1;

        let device = StorageDevice::new(id, driver, model, serial, firmware, timestamp);
        self.devices.insert(id, device);

        self.stats.devices_registered += 1;

        Ok(id)
    }

    /// Unregister a storage device
    pub fn unregister_device(&mut self, id: u32) -> Result<(), StorageError> {
        if self.devices.remove(&id).is_some() {
            self.stats.devices_unregistered += 1;
            Ok(())
        } else {
            Err(StorageError::DeviceNotFound)
        }
    }

    /// Get device by ID
    pub fn get_device(&self, id: u32) -> Option<&StorageDevice> {
        self.devices.get(&id)
    }

    /// Get mutable device by ID
    pub fn get_device_mut(&mut self, id: u32) -> Option<&mut StorageDevice> {
        self.devices.get_mut(&id)
    }

    /// Get all device information
    pub fn get_all_device_info(&self) -> Vec<StorageDeviceInfo> {
        self.devices
            .values()
            .map(|device| device.get_info())
            .collect()
    }

    /// Get device count
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Initialize all devices
    pub fn init_all_devices(&mut self) -> Result<(), StorageError> {
        for device in self.devices.values_mut() {
            device.driver.init()?;
        }
        Ok(())
    }

    /// Read from device
    pub fn read_sectors(
        &mut self,
        device_id: u32,
        start_sector: u64,
        buffer: &mut [u8],
    ) -> Result<usize, StorageError> {
        let device = self
            .devices
            .get_mut(&device_id)
            .ok_or(StorageError::DeviceNotFound)?;

        let result = device.driver.read_sectors(start_sector, buffer);
        device.update_access(crate::time::get_system_time_ms());

        if result.is_ok() {
            self.stats.total_reads += 1;
        }

        result
    }

    /// Write to device
    pub fn write_sectors(
        &mut self,
        device_id: u32,
        start_sector: u64,
        buffer: &[u8],
    ) -> Result<usize, StorageError> {
        let device = self
            .devices
            .get_mut(&device_id)
            .ok_or(StorageError::DeviceNotFound)?;

        let result = device.driver.write_sectors(start_sector, buffer);
        device.update_access(crate::time::get_system_time_ms());

        if result.is_ok() {
            self.stats.total_writes += 1;
        }

        result
    }

    /// Get manager statistics
    pub fn get_stats(&self) -> &StorageManagerStats {
        &self.stats
    }
}

/// Storage manager statistics
#[derive(Debug, Default, Clone)]
pub struct StorageManagerStats {
    pub devices_registered: u64,
    pub devices_unregistered: u64,
    pub total_reads: u64,
    pub total_writes: u64,
    pub errors: u64,
}

/// Global storage driver manager
static STORAGE_MANAGER: RwLock<Option<StorageDriverManager>> = RwLock::new(None);

/// Initialize global storage driver manager
pub fn init_storage_manager() {
    *STORAGE_MANAGER.write() = Some(StorageDriverManager::new());
}

/// Get reference to global storage driver manager
pub fn with_storage_manager<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut StorageDriverManager) -> R,
{
    STORAGE_MANAGER.write().as_mut().map(f)
}

/// High-level storage management functions
pub fn get_storage_device_list() -> Vec<StorageDeviceInfo> {
    with_storage_manager(|manager| manager.get_all_device_info()).unwrap_or_default()
}

pub fn read_storage_sectors(
    device_id: u32,
    start_sector: u64,
    buffer: &mut [u8],
) -> Result<usize, StorageError> {
    with_storage_manager(|manager| manager.read_sectors(device_id, start_sector, buffer))
        .ok_or(StorageError::DeviceNotFound)?
}

pub fn write_storage_sectors(
    device_id: u32,
    start_sector: u64,
    buffer: &[u8],
) -> Result<usize, StorageError> {
    with_storage_manager(|manager| manager.write_sectors(device_id, start_sector, buffer))
        .ok_or(StorageError::DeviceNotFound)?
}

/// Initialize storage subsystem during kernel boot
pub fn init_storage_subsystem() -> Result<detection::DetectionResults, StorageError> {
    detection::detect_and_initialize_storage()
}

// =============================================================================
// UNIFIED BLOCK DEVICE INTERFACE
// =============================================================================

/// Default storage device ID (first available device)
/// This is used by the simplified read/write functions when no device ID is specified
static DEFAULT_DEVICE_ID: spin::RwLock<Option<u32>> = spin::RwLock::new(None);

/// Set the default storage device ID
pub fn set_default_device(device_id: u32) {
    *DEFAULT_DEVICE_ID.write() = Some(device_id);
}

/// Get the default storage device ID
pub fn get_default_device() -> Option<u32> {
    // First try the explicitly set default
    if let Some(id) = *DEFAULT_DEVICE_ID.read() {
        return Some(id);
    }

    // Otherwise, use the first available device
    with_storage_manager(|manager| manager.get_all_device_info().first().map(|info| info.id))
        .flatten()
}

/// Read sectors from storage using unified interface (uses default device)
///
/// # Arguments
/// * `sector` - Starting sector number
/// * `count` - Number of sectors to read
/// * `buffer` - Buffer to store read data (must be at least count * 512 bytes)
///
/// # Returns
/// * `Ok(())` - Read successful
/// * `Err(StorageError)` - Read failed
pub fn read_sectors(sector: u64, count: u32, buffer: &mut [u8]) -> Result<(), StorageError> {
    let device_id = get_default_device().ok_or(StorageError::DeviceNotFound)?;

    let sector_size = with_storage_manager(|manager| {
        manager
            .get_device(device_id)
            .map(|device| device.driver.capabilities().sector_size as usize)
    })
    .flatten()
    .ok_or(StorageError::DeviceNotFound)?;
    if sector_size == 0 {
        return Err(StorageError::HardwareError);
    }
    let required_size = (count as usize) * sector_size;

    if buffer.len() < required_size {
        return Err(StorageError::BufferTooSmall);
    }

    // Read in chunks if needed (handle large transfers)
    let max_sectors_per_transfer = 256u32; // Common max for most controllers
    let mut current_sector = sector;
    let mut sectors_remaining = count;
    let mut buffer_offset = 0usize;

    while sectors_remaining > 0 {
        let transfer_sectors = core::cmp::min(sectors_remaining, max_sectors_per_transfer);
        let transfer_size = (transfer_sectors as usize) * sector_size;
        let transfer_buffer = &mut buffer[buffer_offset..buffer_offset + transfer_size];

        let bytes_read = with_storage_manager(|manager| {
            manager.read_sectors(device_id, current_sector, transfer_buffer)
        })
        .ok_or(StorageError::DeviceNotFound)??;

        if bytes_read != transfer_size {
            return Err(StorageError::MediaError);
        }

        current_sector += transfer_sectors as u64;
        sectors_remaining -= transfer_sectors;
        buffer_offset += transfer_size;
    }

    Ok(())
}

/// Write sectors to storage using unified interface (uses default device)
///
/// # Arguments
/// * `sector` - Starting sector number
/// * `count` - Number of sectors to write
/// * `buffer` - Buffer containing data to write (must be at least count * 512 bytes)
///
/// # Returns
/// * `Ok(())` - Write successful
/// * `Err(StorageError)` - Write failed
pub fn write_sectors(sector: u64, count: u32, buffer: &[u8]) -> Result<(), StorageError> {
    let device_id = get_default_device().ok_or(StorageError::DeviceNotFound)?;

    let sector_size = with_storage_manager(|manager| {
        manager
            .get_device(device_id)
            .map(|device| device.driver.capabilities().sector_size as usize)
    })
    .flatten()
    .ok_or(StorageError::DeviceNotFound)?;
    if sector_size == 0 {
        return Err(StorageError::HardwareError);
    }
    let required_size = (count as usize) * sector_size;

    if buffer.len() < required_size {
        return Err(StorageError::BufferTooSmall);
    }

    // Write in chunks if needed (handle large transfers)
    let max_sectors_per_transfer = 256u32; // Common max for most controllers
    let mut current_sector = sector;
    let mut sectors_remaining = count;
    let mut buffer_offset = 0usize;

    while sectors_remaining > 0 {
        let transfer_sectors = core::cmp::min(sectors_remaining, max_sectors_per_transfer);
        let transfer_size = (transfer_sectors as usize) * sector_size;
        let transfer_buffer = &buffer[buffer_offset..buffer_offset + transfer_size];

        let bytes_written = with_storage_manager(|manager| {
            manager.write_sectors(device_id, current_sector, transfer_buffer)
        })
        .ok_or(StorageError::DeviceNotFound)??;

        if bytes_written != transfer_size {
            return Err(StorageError::MediaError);
        }

        current_sector += transfer_sectors as u64;
        sectors_remaining -= transfer_sectors;
        buffer_offset += transfer_size;
    }

    Ok(())
}

/// Flush all pending writes to storage
pub fn flush_storage() -> Result<(), StorageError> {
    let device_id = get_default_device().ok_or(StorageError::DeviceNotFound)?;

    with_storage_manager(|manager| {
        if let Some(device) = manager.get_device_mut(device_id) {
            device.driver.flush()
        } else {
            Err(StorageError::DeviceNotFound)
        }
    })
    .ok_or(StorageError::DeviceNotFound)?
}

// =============================================================================
// MULTI-DEVICE BLOCK DEVICE INTERFACE
// =============================================================================

/// Block device abstraction for unified access to multiple storage devices
#[derive(Debug)]
pub struct BlockDevice {
    device_id: u32,
    sector_size: u32,
    total_sectors: u64,
    device_type: StorageDeviceType,
}

impl BlockDevice {
    /// Create a new block device handle
    pub fn new(device_id: u32) -> Result<Self, StorageError> {
        with_storage_manager(|manager| {
            if let Some(device) = manager.get_device(device_id) {
                let caps = device.driver.capabilities();
                Ok(BlockDevice {
                    device_id,
                    sector_size: caps.sector_size,
                    total_sectors: caps.capacity_bytes / caps.sector_size as u64,
                    device_type: device.driver.device_type(),
                })
            } else {
                Err(StorageError::DeviceNotFound)
            }
        })
        .ok_or(StorageError::DeviceNotFound)?
    }

    /// Get first available block device
    pub fn first() -> Result<Self, StorageError> {
        let device_id = get_default_device().ok_or(StorageError::DeviceNotFound)?;
        Self::new(device_id)
    }

    /// Get device ID
    pub fn device_id(&self) -> u32 {
        self.device_id
    }

    /// Get sector size in bytes
    pub fn sector_size(&self) -> u32 {
        self.sector_size
    }

    /// Get total number of sectors
    pub fn total_sectors(&self) -> u64 {
        self.total_sectors
    }

    /// Get device type
    pub fn device_type(&self) -> StorageDeviceType {
        self.device_type
    }

    /// Get total capacity in bytes
    pub fn capacity_bytes(&self) -> u64 {
        self.total_sectors * self.sector_size as u64
    }

    /// Read sectors from this device
    pub fn read(&self, sector: u64, count: u32, buffer: &mut [u8]) -> Result<(), StorageError> {
        let required_size = (count as usize) * (self.sector_size as usize);
        if buffer.len() < required_size {
            return Err(StorageError::BufferTooSmall);
        }

        if sector + count as u64 > self.total_sectors {
            return Err(StorageError::InvalidSector);
        }

        let bytes_read = read_storage_sectors(self.device_id, sector, buffer)?;
        if bytes_read != required_size {
            return Err(StorageError::MediaError);
        }

        Ok(())
    }

    /// Write sectors to this device
    pub fn write(&self, sector: u64, count: u32, buffer: &[u8]) -> Result<(), StorageError> {
        let required_size = (count as usize) * (self.sector_size as usize);
        if buffer.len() < required_size {
            return Err(StorageError::BufferTooSmall);
        }

        if sector + count as u64 > self.total_sectors {
            return Err(StorageError::InvalidSector);
        }

        let bytes_written = write_storage_sectors(self.device_id, sector, buffer)?;
        if bytes_written != required_size {
            return Err(StorageError::MediaError);
        }

        Ok(())
    }

    /// Flush pending writes
    pub fn flush(&self) -> Result<(), StorageError> {
        with_storage_manager(|manager| {
            if let Some(device) = manager.get_device_mut(self.device_id) {
                device.driver.flush()
            } else {
                Err(StorageError::DeviceNotFound)
            }
        })
        .ok_or(StorageError::DeviceNotFound)?
    }

    /// Get device statistics
    pub fn statistics(&self) -> Result<StorageStats, StorageError> {
        with_storage_manager(|manager| {
            if let Some(device) = manager.get_device(self.device_id) {
                Ok(device.driver.get_stats())
            } else {
                Err(StorageError::DeviceNotFound)
            }
        })
        .ok_or(StorageError::DeviceNotFound)?
    }
}

/// List all available block devices
pub fn list_block_devices() -> Vec<BlockDevice> {
    with_storage_manager(|manager| {
        manager
            .get_all_device_info()
            .iter()
            .filter_map(|info| {
                manager.get_device(info.id).map(|device| {
                    let caps = device.driver.capabilities();
                    BlockDevice {
                        device_id: info.id,
                        sector_size: caps.sector_size,
                        total_sectors: caps.capacity_bytes / caps.sector_size as u64,
                        device_type: device.driver.device_type(),
                    }
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Get block device by type
pub fn get_device_by_type(device_type: StorageDeviceType) -> Option<BlockDevice> {
    with_storage_manager(|manager| {
        manager
            .get_all_device_info()
            .iter()
            .find(|info| info.device_type == device_type)
            .and_then(|info| {
                manager.get_device(info.id).map(|device| {
                    let caps = device.driver.capabilities();
                    BlockDevice {
                        device_id: info.id,
                        sector_size: caps.sector_size,
                        total_sectors: caps.capacity_bytes / caps.sector_size as u64,
                        device_type: device.driver.device_type(),
                    }
                })
            })
    })
    .flatten()
}

// =============================================================================
// PARTITION TABLE SUPPORT
// =============================================================================

/// MBR partition entry
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MbrPartitionEntry {
    pub bootable: u8,
    pub start_chs: [u8; 3],
    pub partition_type: u8,
    pub end_chs: [u8; 3],
    pub start_lba: u32,
    pub sector_count: u32,
}

/// GPT partition entry (simplified)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GptPartitionEntry {
    pub type_guid: [u8; 16],
    pub partition_guid: [u8; 16],
    pub start_lba: u64,
    pub end_lba: u64,
    pub attributes: u64,
    pub name: [u16; 36],
}

/// Partition information
#[derive(Debug, Clone)]
pub struct PartitionInfo {
    pub device_id: u32,
    pub partition_number: u32,
    pub start_sector: u64,
    pub sector_count: u64,
    pub partition_type: PartitionType,
    pub bootable: bool,
    pub name: Option<String>,
}

/// Partition type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionType {
    Empty,
    Fat12,
    Fat16,
    Fat32,
    ExtendedBoot,
    Ntfs,
    LinuxSwap,
    LinuxNative,
    LinuxLvm,
    Efi,
    Unknown(u8),
}

impl From<u8> for PartitionType {
    fn from(type_byte: u8) -> Self {
        match type_byte {
            0x00 => PartitionType::Empty,
            0x01 => PartitionType::Fat12,
            0x04 | 0x06 | 0x0E => PartitionType::Fat16,
            0x0B | 0x0C => PartitionType::Fat32,
            0x05 | 0x0F => PartitionType::ExtendedBoot,
            0x07 => PartitionType::Ntfs,
            0x82 => PartitionType::LinuxSwap,
            0x83 => PartitionType::LinuxNative,
            0x8E => PartitionType::LinuxLvm,
            0xEF => PartitionType::Efi,
            other => PartitionType::Unknown(other),
        }
    }
}

/// Read MBR partition table
pub fn read_mbr_partitions(device_id: u32) -> Result<Vec<PartitionInfo>, StorageError> {
    let mut buffer = [0u8; 512];
    read_storage_sectors(device_id, 0, &mut buffer)?;

    // Check MBR signature
    if buffer[510] != 0x55 || buffer[511] != 0xAA {
        return Err(StorageError::MediaError);
    }

    let mut partitions = Vec::new();

    // Parse partition entries (offset 446, 4 entries of 16 bytes each)
    for i in 0..4 {
        let offset = 446 + i * 16;
        let entry = unsafe {
            core::ptr::read_unaligned(buffer.as_ptr().add(offset) as *const MbrPartitionEntry)
        };

        // Skip empty partitions
        if entry.partition_type == 0 || entry.sector_count == 0 {
            continue;
        }

        partitions.push(PartitionInfo {
            device_id,
            partition_number: i as u32 + 1,
            start_sector: entry.start_lba as u64,
            sector_count: entry.sector_count as u64,
            partition_type: PartitionType::from(entry.partition_type),
            bootable: entry.bootable == 0x80,
            name: None,
        });
    }

    Ok(partitions)
}

/// Check if device has GPT partition table
pub fn is_gpt_device(device_id: u32) -> Result<bool, StorageError> {
    let mut buffer = [0u8; 512];

    // Read LBA 1 (GPT header)
    read_storage_sectors(device_id, 1, &mut buffer)?;

    // Check GPT signature "EFI PART"
    Ok(&buffer[0..8] == b"EFI PART")
}

// =============================================================================
// STORAGE SUBSYSTEM CONTROL
// =============================================================================

/// Storage subsystem status
#[derive(Debug, Clone)]
pub struct StorageSubsystemStatus {
    pub initialized: bool,
    pub device_count: usize,
    pub total_capacity_bytes: u64,
    pub manager_stats: StorageManagerStats,
}

/// Get storage subsystem status
pub fn get_subsystem_status() -> StorageSubsystemStatus {
    with_storage_manager(|manager| {
        let devices = manager.get_all_device_info();
        let total_capacity: u64 = devices.iter().map(|d| d.capabilities.capacity_bytes).sum();

        StorageSubsystemStatus {
            initialized: true,
            device_count: devices.len(),
            total_capacity_bytes: total_capacity,
            manager_stats: manager.get_stats().clone(),
        }
    })
    .unwrap_or(StorageSubsystemStatus {
        initialized: false,
        device_count: 0,
        total_capacity_bytes: 0,
        manager_stats: StorageManagerStats::default(),
    })
}

/// Reset a storage device
pub fn reset_device(device_id: u32) -> Result<(), StorageError> {
    with_storage_manager(|manager| {
        if let Some(device) = manager.get_device_mut(device_id) {
            device.driver.reset()
        } else {
            Err(StorageError::DeviceNotFound)
        }
    })
    .ok_or(StorageError::DeviceNotFound)?
}

/// Put a storage device in standby mode
pub fn standby_device(device_id: u32) -> Result<(), StorageError> {
    with_storage_manager(|manager| {
        if let Some(device) = manager.get_device_mut(device_id) {
            device.driver.standby()
        } else {
            Err(StorageError::DeviceNotFound)
        }
    })
    .ok_or(StorageError::DeviceNotFound)?
}

/// Wake a storage device from standby
pub fn wake_device(device_id: u32) -> Result<(), StorageError> {
    with_storage_manager(|manager| {
        if let Some(device) = manager.get_device_mut(device_id) {
            device.driver.wake()
        } else {
            Err(StorageError::DeviceNotFound)
        }
    })
    .ok_or(StorageError::DeviceNotFound)?
}

/// Get SMART data from a device
pub fn get_device_smart_data(device_id: u32) -> Result<Vec<u8>, StorageError> {
    with_storage_manager(|manager| {
        if let Some(device) = manager.get_device_mut(device_id) {
            device.driver.get_smart_data()
        } else {
            Err(StorageError::DeviceNotFound)
        }
    })
    .ok_or(StorageError::DeviceNotFound)?
}
