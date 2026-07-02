//! # Filesystem Interface for Storage Drivers
//!
//! Provides a unified interface between storage drivers and filesystem layers.
//! Supports block-level operations, partition management, and filesystem detection.

use super::StorageError;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Partition table types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionTableType {
    /// Master Boot Record (MBR/DOS)
    Mbr,
    /// GUID Partition Table (GPT)
    Gpt,
    /// No partition table (whole disk)
    None,
    /// Unknown/Unrecognized
    Unknown,
}

/// Filesystem types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemType {
    /// FAT12 filesystem
    Fat12,
    /// FAT16 filesystem
    Fat16,
    /// FAT32 filesystem
    Fat32,
    /// Extended FAT (exFAT)
    ExFat,
    /// NTFS filesystem
    Ntfs,
    /// ext2 filesystem
    Ext2,
    /// ext3 filesystem
    Ext3,
    /// ext4 filesystem
    Ext4,
    /// ISO 9660 (CD/DVD)
    Iso9660,
    /// UDF (Universal Disk Format)
    Udf,
    /// HFS+ (Mac OS)
    HfsPlus,
    /// Raw/Unformatted
    Raw,
    /// Unknown filesystem
    Unknown,
}

impl core::fmt::Display for FilesystemType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FilesystemType::Fat12 => write!(f, "FAT12"),
            FilesystemType::Fat16 => write!(f, "FAT16"),
            FilesystemType::Fat32 => write!(f, "FAT32"),
            FilesystemType::ExFat => write!(f, "exFAT"),
            FilesystemType::Ntfs => write!(f, "NTFS"),
            FilesystemType::Ext2 => write!(f, "ext2"),
            FilesystemType::Ext3 => write!(f, "ext3"),
            FilesystemType::Ext4 => write!(f, "ext4"),
            FilesystemType::Iso9660 => write!(f, "ISO 9660"),
            FilesystemType::Udf => write!(f, "UDF"),
            FilesystemType::HfsPlus => write!(f, "HFS+"),
            FilesystemType::Raw => write!(f, "Raw"),
            FilesystemType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Partition information
#[derive(Debug, Clone)]
pub struct PartitionInfo {
    /// Partition number (0-based)
    pub number: u8,
    /// Start sector (LBA)
    pub start_sector: u64,
    /// Size in sectors
    pub sector_count: u64,
    /// Partition type/ID
    pub partition_type: u8,
    /// Filesystem type
    pub filesystem_type: FilesystemType,
    /// Partition label/name
    pub label: String,
    /// Whether partition is bootable
    pub bootable: bool,
    /// Whether partition is active
    pub active: bool,
}

impl PartitionInfo {
    /// Get partition size in bytes
    pub fn size_bytes(&self) -> u64 {
        self.sector_count * 512 // Assume 512-byte sectors
    }

    /// Get partition size in MB
    pub fn size_mb(&self) -> u64 {
        self.size_bytes() / (1024 * 1024)
    }

    /// Get partition size in GB
    pub fn size_gb(&self) -> f64 {
        self.size_bytes() as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}

/// Master Boot Record structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MasterBootRecord {
    /// Bootstrap code
    pub bootstrap: [u8; 446],
    /// Partition table entries
    pub partitions: [MbrPartitionEntry; 4],
    /// Boot signature (0x55AA)
    pub signature: u16,
}

/// MBR partition entry
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MbrPartitionEntry {
    /// Boot indicator (0x80 = bootable)
    pub boot_indicator: u8,
    /// Starting CHS address
    pub start_chs: [u8; 3],
    /// Partition type
    pub partition_type: u8,
    /// Ending CHS address
    pub end_chs: [u8; 3],
    /// Starting LBA address
    pub start_lba: u32,
    /// Size in sectors
    pub size_sectors: u32,
}

/// Safely parse a 512-byte MBR buffer into a `MasterBootRecord` without
/// relying on `unsafe` pointer casts. The MBR layout is:
///   [0..446)   bootstrap code
///   [446..510) four 16-byte partition entries
///   [510..512) 0x55AA signature
fn parse_mbr(buf: &[u8; 512]) -> MasterBootRecord {
    let mut bootstrap = [0u8; 446];
    bootstrap.copy_from_slice(&buf[..446]);

    let mut partitions = [MbrPartitionEntry {
        boot_indicator: 0,
        start_chs: [0; 3],
        partition_type: 0,
        end_chs: [0; 3],
        start_lba: 0,
        size_sectors: 0,
    }; 4];

    for (i, entry) in partitions.iter_mut().enumerate() {
        let off = 446 + i * 16;
        let chunk = &buf[off..off + 16];
        entry.boot_indicator = chunk[0];
        entry.start_chs.copy_from_slice(&chunk[1..4]);
        entry.partition_type = chunk[4];
        entry.end_chs.copy_from_slice(&chunk[5..8]);
        entry.start_lba = u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]);
        entry.size_sectors = u32::from_le_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]);
    }

    let signature = u16::from_le_bytes([buf[510], buf[511]]);

    MasterBootRecord {
        bootstrap,
        partitions,
        signature,
    }
}

impl MbrPartitionEntry {
    /// Check if partition entry is valid
    pub fn is_valid(&self) -> bool {
        self.partition_type != 0 && self.size_sectors > 0
    }

    /// Check if partition is bootable
    pub fn is_bootable(&self) -> bool {
        self.boot_indicator == 0x80
    }

    /// Get filesystem type from partition type
    pub fn get_filesystem_type(&self) -> FilesystemType {
        match self.partition_type {
            0x01 => FilesystemType::Fat12,
            0x04 | 0x06 | 0x0E => FilesystemType::Fat16,
            0x0B | 0x0C => FilesystemType::Fat32,
            0x07 => FilesystemType::Ntfs,
            0x83 => FilesystemType::Ext4, // Linux partition (could be ext2/3/4)
            0x82 => FilesystemType::Raw,  // Linux swap
            0xEF => FilesystemType::Fat32, // EFI System Partition
            _ => FilesystemType::Unknown,
        }
    }
}

/// Block device interface for filesystem access
pub trait BlockDevice: Send + Sync {
    /// Read blocks from the device
    fn read_blocks(&mut self, start_block: u64, buffer: &mut [u8]) -> Result<usize, StorageError>;

    /// Write blocks to the device
    fn write_blocks(&mut self, start_block: u64, buffer: &[u8]) -> Result<usize, StorageError>;

    /// Flush any pending writes
    fn flush(&mut self) -> Result<(), StorageError>;

    /// Get block size in bytes
    fn block_size(&self) -> u32;

    /// Get total number of blocks
    fn block_count(&self) -> u64;

    /// Check if device is read-only
    fn is_read_only(&self) -> bool;
}

/// Storage device wrapper implementing BlockDevice
pub struct StorageBlockDevice {
    device_id: u32,
    block_size: u32,
    block_count: u64,
    read_only: bool,
}

impl StorageBlockDevice {
    pub fn new(device_id: u32, block_size: u32, block_count: u64, read_only: bool) -> Self {
        Self {
            device_id,
            block_size,
            block_count,
            read_only,
        }
    }

    pub fn device_id(&self) -> u32 {
        self.device_id
    }
}

impl BlockDevice for StorageBlockDevice {
    fn read_blocks(&mut self, start_block: u64, buffer: &mut [u8]) -> Result<usize, StorageError> {
        // Convert blocks to sectors (assuming block_size >= 512)
        let sectors_per_block = self.block_size / 512;
        let start_sector = start_block * sectors_per_block as u64;

        super::read_storage_sectors(self.device_id, start_sector, buffer)
    }

    fn write_blocks(&mut self, start_block: u64, buffer: &[u8]) -> Result<usize, StorageError> {
        if self.read_only {
            return Err(StorageError::PermissionDenied);
        }

        let sectors_per_block = self.block_size / 512;
        let start_sector = start_block * sectors_per_block as u64;

        super::write_storage_sectors(self.device_id, start_sector, buffer)
    }

    fn flush(&mut self) -> Result<(), StorageError> {
        // Use storage manager to flush device
        super::with_storage_manager(|manager| {
            if let Some(device) = manager.get_device_mut(self.device_id) {
                device.driver.flush()
            } else {
                Err(StorageError::DeviceNotFound)
            }
        })
        .unwrap_or(Err(StorageError::DeviceNotFound))
    }

    fn block_size(&self) -> u32 {
        self.block_size
    }

    fn block_count(&self) -> u64 {
        self.block_count
    }

    fn is_read_only(&self) -> bool {
        self.read_only
    }
}

/// Partition manager for handling disk partitions
pub struct PartitionManager {
    partitions: BTreeMap<u32, Vec<PartitionInfo>>,
}

impl PartitionManager {
    pub fn new() -> Self {
        Self {
            partitions: BTreeMap::new(),
        }
    }

    /// Scan device for partitions
    pub fn scan_device(&mut self, device_id: u32) -> Result<Vec<PartitionInfo>, StorageError> {
        let mut buffer = [0u8; 512];

        // Read MBR (sector 0)
        super::read_storage_sectors(device_id, 0, &mut buffer)?;

        let mbr = parse_mbr(&buffer);

        // Check MBR signature
        if mbr.signature != 0xAA55 {
            // No valid MBR, check for filesystem directly
            let fs_type = self.detect_filesystem(&buffer);
            let partition = PartitionInfo {
                number: 0,
                start_sector: 0,
                sector_count: 0, // Would need to get from device info
                partition_type: 0,
                filesystem_type: fs_type,
                label: String::new(),
                bootable: false,
                active: true,
            };

            let partitions = vec![partition];
            self.partitions.insert(device_id, partitions.clone());
            return Ok(partitions);
        }

        // Parse MBR partitions
        let mut partitions = Vec::new();
        for (i, entry) in mbr.partitions.iter().enumerate() {
            if entry.is_valid() {
                // Read first sector of partition to detect filesystem
                let mut part_buffer = [0u8; 512];
                if super::read_storage_sectors(device_id, entry.start_lba as u64, &mut part_buffer)
                    .is_ok()
                {
                    let fs_type = self.detect_filesystem(&part_buffer);

                    let partition = PartitionInfo {
                        number: i as u8,
                        start_sector: entry.start_lba as u64,
                        sector_count: entry.size_sectors as u64,
                        partition_type: entry.partition_type,
                        filesystem_type: fs_type,
                        label: self.get_partition_label(&part_buffer, fs_type),
                        bootable: entry.is_bootable(),
                        active: true,
                    };

                    partitions.push(partition);
                }
            }
        }

        self.partitions.insert(device_id, partitions.clone());
        Ok(partitions)
    }

    /// Detect filesystem type from boot sector
    fn detect_filesystem(&self, buffer: &[u8]) -> FilesystemType {
        // Check for FAT filesystem
        if buffer.len() >= 62 {
            // FAT12/16 signature
            if &buffer[54..62] == b"FAT12   " {
                return FilesystemType::Fat12;
            }
            if &buffer[54..62] == b"FAT16   " {
                return FilesystemType::Fat16;
            }
        }

        if buffer.len() >= 90 {
            // FAT32 signature
            if &buffer[82..90] == b"FAT32   " {
                return FilesystemType::Fat32;
            }
        }

        // Check for NTFS
        if buffer.len() >= 8 && &buffer[3..8] == b"NTFS " {
            return FilesystemType::Ntfs;
        }

        // Check for ext2/3/4
        if buffer.len() >= 1080 {
            let ext_magic = u16::from_le_bytes([buffer[1080], buffer[1081]]);
            if ext_magic == 0xEF53 {
                // Could be ext2, ext3, or ext4 - would need to check features
                return FilesystemType::Ext4;
            }
        }

        // Check for ISO 9660
        if buffer.len() >= 32 && &buffer[1..6] == b"CD001" {
            return FilesystemType::Iso9660;
        }

        // Check for exFAT
        if buffer.len() >= 11 && &buffer[3..11] == b"EXFAT   " {
            return FilesystemType::ExFat;
        }

        FilesystemType::Unknown
    }

    /// Get partition label from boot sector
    fn get_partition_label(&self, buffer: &[u8], fs_type: FilesystemType) -> String {
        match fs_type {
            FilesystemType::Fat12 | FilesystemType::Fat16 => {
                if buffer.len() >= 54 {
                    let label_bytes = &buffer[43..54];
                    String::from_utf8_lossy(label_bytes).trim().to_string()
                } else {
                    String::new()
                }
            }
            FilesystemType::Fat32 => {
                if buffer.len() >= 82 {
                    let label_bytes = &buffer[71..82];
                    String::from_utf8_lossy(label_bytes).trim().to_string()
                } else {
                    String::new()
                }
            }
            FilesystemType::Ntfs => {
                // NTFS volume label is in a different location
                String::new()
            }
            _ => String::new(),
        }
    }

    /// Get partitions for a device
    pub fn get_partitions(&self, device_id: u32) -> Option<&Vec<PartitionInfo>> {
        self.partitions.get(&device_id)
    }

    /// Create a block device for a specific partition
    pub fn create_partition_block_device(
        &self,
        device_id: u32,
        partition_num: u8,
    ) -> Result<PartitionBlockDevice, StorageError> {
        let partitions = self
            .get_partitions(device_id)
            .ok_or(StorageError::DeviceNotFound)?;

        let partition = partitions
            .iter()
            .find(|p| p.number == partition_num)
            .ok_or(StorageError::InvalidSector)?;

        Ok(PartitionBlockDevice::new(
            device_id,
            partition.start_sector,
            partition.sector_count,
            512, // Assume 512-byte sectors
        ))
    }
}

/// Block device implementation for a specific partition
pub struct PartitionBlockDevice {
    device_id: u32,
    start_sector: u64,
    sector_count: u64,
    block_size: u32,
}

impl PartitionBlockDevice {
    pub fn new(device_id: u32, start_sector: u64, sector_count: u64, block_size: u32) -> Self {
        Self {
            device_id,
            start_sector,
            sector_count,
            block_size,
        }
    }
}

impl BlockDevice for PartitionBlockDevice {
    fn read_blocks(&mut self, start_block: u64, buffer: &mut [u8]) -> Result<usize, StorageError> {
        let sectors_per_block = self.block_size / 512;
        let start_sector = self.start_sector + (start_block * sectors_per_block as u64);

        // Check bounds
        if start_sector >= self.start_sector + self.sector_count {
            return Err(StorageError::InvalidSector);
        }

        super::read_storage_sectors(self.device_id, start_sector, buffer)
    }

    fn write_blocks(&mut self, start_block: u64, buffer: &[u8]) -> Result<usize, StorageError> {
        let sectors_per_block = self.block_size / 512;
        let start_sector = self.start_sector + (start_block * sectors_per_block as u64);

        // Check bounds
        if start_sector >= self.start_sector + self.sector_count {
            return Err(StorageError::InvalidSector);
        }

        super::write_storage_sectors(self.device_id, start_sector, buffer)
    }

    fn flush(&mut self) -> Result<(), StorageError> {
        super::with_storage_manager(|manager| {
            if let Some(device) = manager.get_device_mut(self.device_id) {
                device.driver.flush()
            } else {
                Err(StorageError::DeviceNotFound)
            }
        })
        .unwrap_or(Err(StorageError::DeviceNotFound))
    }

    fn block_size(&self) -> u32 {
        self.block_size
    }

    fn block_count(&self) -> u64 {
        self.sector_count / (self.block_size as u64 / 512)
    }

    fn is_read_only(&self) -> bool {
        // Note: Device read-only status checking requires integration with storage driver capabilities.
        // Future enhancement will query the underlying storage device for write protection status.
        false
    }
}

/// Filesystem detection and mounting interface
pub struct FilesystemInterface {
    partition_manager: PartitionManager,
    mounted_filesystems: BTreeMap<String, u32>, // mount_point -> device_id
}

impl FilesystemInterface {
    pub fn new() -> Self {
        Self {
            partition_manager: PartitionManager::new(),
            mounted_filesystems: BTreeMap::new(),
        }
    }

    /// Scan all storage devices for partitions
    pub fn scan_all_devices(&mut self) -> Result<Vec<(u32, Vec<PartitionInfo>)>, StorageError> {
        let mut results = Vec::new();
        let devices = super::get_storage_device_list();

        for device_info in devices {
            if let Ok(partitions) = self.partition_manager.scan_device(device_info.id) {
                results.push((device_info.id, partitions));
            }
        }

        Ok(results)
    }

    /// Get partition information for a device
    pub fn get_device_partitions(&self, device_id: u32) -> Option<&Vec<PartitionInfo>> {
        self.partition_manager.get_partitions(device_id)
    }

    /// Create block device for filesystem access
    pub fn create_block_device(
        &self,
        device_id: u32,
        partition_num: Option<u8>,
    ) -> Result<Box<dyn BlockDevice>, StorageError> {
        if let Some(part_num) = partition_num {
            // Access specific partition
            let part_device = self
                .partition_manager
                .create_partition_block_device(device_id, part_num)?;
            Ok(Box::new(part_device))
        } else {
            // Access whole device
            let device_info = super::get_storage_device_list()
                .into_iter()
                .find(|info| info.id == device_id)
                .ok_or(StorageError::DeviceNotFound)?;

            let block_size = device_info.capabilities.sector_size;
            let block_count = device_info.capabilities.capacity_bytes / block_size as u64;

            let storage_device = StorageBlockDevice::new(
                device_id,
                block_size,
                block_count,
                false, // Note: Read-only detection planned for future release
            );

            Ok(Box::new(storage_device))
        }
    }

    /// Mount a filesystem
    ///
    /// # Arguments
    ///
    /// * `device_id` - Storage device identifier
    /// * `partition_num` - Optional partition number to mount
    /// * `mount_point` - Virtual filesystem mount point path
    /// * `_fs_type` - Optional filesystem type hint
    ///
    /// # Implementation Status
    ///
    /// Current implementation registers mount points without full filesystem integration.
    /// Complete filesystem mounting with VFS integration is planned for future releases,
    /// which will include:
    /// - Superblock reading and validation
    /// - Inode cache initialization
    /// - Directory tree integration with VFS
    /// - Mount option processing
    pub fn mount_filesystem(
        &mut self,
        device_id: u32,
        _partition_num: Option<u8>,
        mount_point: String,
        _fs_type: Option<FilesystemType>,
    ) -> Result<(), StorageError> {
        // Register mount point for tracking
        self.mounted_filesystems.insert(mount_point, device_id);
        Ok(())
    }

    /// Unmount a filesystem
    pub fn unmount_filesystem(&mut self, mount_point: &str) -> Result<(), StorageError> {
        if self.mounted_filesystems.remove(mount_point).is_some() {
            Ok(())
        } else {
            Err(StorageError::DeviceNotFound)
        }
    }

    /// Get list of mounted filesystems
    pub fn get_mounted_filesystems(&self) -> Vec<(String, u32)> {
        self.mounted_filesystems
            .iter()
            .map(|(mount_point, device_id)| (mount_point.clone(), *device_id))
            .collect()
    }
}

/// Global filesystem interface
static FILESYSTEM_INTERFACE: spin::Mutex<Option<FilesystemInterface>> = spin::Mutex::new(None);

/// Initialize filesystem interface
pub fn init_filesystem_interface() {
    *FILESYSTEM_INTERFACE.lock() = Some(FilesystemInterface::new());
}

/// Run `f` with the global filesystem interface, if initialized.
pub fn with_filesystem_interface<R>(f: impl FnOnce(&mut FilesystemInterface) -> R) -> Option<R> {
    FILESYSTEM_INTERFACE.lock().as_mut().map(f)
}

/// Scan all storage devices for filesystems
pub fn scan_all_storage_filesystems() -> Result<Vec<(u32, Vec<PartitionInfo>)>, StorageError> {
    with_filesystem_interface(|fs_interface| fs_interface.scan_all_devices())
        .unwrap_or(Err(StorageError::DeviceNotFound))
}
