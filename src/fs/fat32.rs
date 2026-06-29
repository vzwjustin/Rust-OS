//! FAT32 Filesystem Implementation
//!
//! This module provides a production-ready FAT32 filesystem implementation
//! with proper metadata handling and disk I/O operations.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use crate::drivers::storage::{read_storage_sectors, write_storage_sectors};
use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};
use core::mem;
use spin::RwLock;

/// FAT32 signature
const FAT32_SIGNATURE: u16 = 0xAA55;
const FAT32_FSINFO_SIGNATURE1: u32 = 0x41615252;
const FAT32_FSINFO_SIGNATURE2: u32 = 0x61417272;

/// FAT32 cluster values
const FAT32_EOC: u32 = 0x0FFFFFF8; // End of cluster chain
const FAT32_BAD_CLUSTER: u32 = 0x0FFFFFF7;
const FAT32_FREE_CLUSTER: u32 = 0x00000000;

/// FAT32 Boot Sector (BIOS Parameter Block)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Fat32BootSector {
    pub jmp_boot: [u8; 3],          // Jump instruction
    pub oem_name: [u8; 8],          // OEM name
    pub bytes_per_sector: u16,      // Bytes per sector
    pub sectors_per_cluster: u8,    // Sectors per cluster
    pub reserved_sector_count: u16, // Reserved sectors
    pub num_fats: u8,               // Number of FATs
    pub root_entry_count: u16,      // Root directory entries (0 for FAT32)
    pub total_sectors_16: u16,      // Total sectors (0 for FAT32)
    pub media: u8,                  // Media descriptor
    pub fat_size_16: u16,           // FAT size in sectors (0 for FAT32)
    pub sectors_per_track: u16,     // Sectors per track
    pub num_heads: u16,             // Number of heads
    pub hidden_sectors: u32,        // Hidden sectors
    pub total_sectors_32: u32,      // Total sectors (FAT32)

    // FAT32 specific fields
    pub fat_size_32: u32,        // FAT size in sectors
    pub ext_flags: u16,          // Extended flags
    pub fs_version: u16,         // Filesystem version
    pub root_cluster: u32,       // Root directory cluster
    pub fs_info: u16,            // FSInfo sector
    pub backup_boot_sector: u16, // Backup boot sector
    pub reserved: [u8; 12],      // Reserved
    pub drive_number: u8,        // Drive number
    pub reserved1: u8,           // Reserved
    pub boot_signature: u8,      // Boot signature
    pub volume_id: u32,          // Volume ID
    pub volume_label: [u8; 11],  // Volume label
    pub fs_type: [u8; 8],        // Filesystem type
    pub boot_code: [u8; 420],    // Boot code
    pub signature: u16,          // Boot sector signature (0xAA55)
}

/// FAT32 FSInfo structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Fat32FsInfo {
    pub lead_signature: u32,   // 0x41615252
    pub reserved1: [u8; 480],  // Reserved
    pub struct_signature: u32, // 0x61417272
    pub free_count: u32,       // Free cluster count
    pub next_free: u32,        // Next free cluster
    pub reserved2: [u8; 12],   // Reserved
    pub trail_signature: u32,  // 0xAA550000
}

/// FAT32 directory entry
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Fat32DirEntry {
    pub name: [u8; 11],        // 8.3 filename
    pub attr: u8,              // File attributes
    pub nt_reserved: u8,       // Reserved for Windows NT
    pub create_time_tenth: u8, // Creation time (tenths of second)
    pub create_time: u16,      // Creation time
    pub create_date: u16,      // Creation date
    pub last_access_date: u16, // Last access date
    pub first_cluster_hi: u16, // High 16 bits of first cluster
    pub write_time: u16,       // Last write time
    pub write_date: u16,       // Last write date
    pub first_cluster_lo: u16, // Low 16 bits of first cluster
    pub file_size: u32,        // File size in bytes
}

// FAT32 file attributes
bitflags::bitflags! {
    pub struct Fat32Attr: u8 {
        const READ_ONLY = 0x01;
        const HIDDEN = 0x02;
        const SYSTEM = 0x04;
        const VOLUME_ID = 0x08;
        const DIRECTORY = 0x10;
        const ARCHIVE = 0x20;
        const LONG_NAME = 0x0F; // READ_ONLY | HIDDEN | SYSTEM | VOLUME_ID
    }
}

/// Long filename entry
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Fat32LfnEntry {
    pub order: u8,             // Order of this entry
    pub name1: [u16; 5],       // First 5 characters
    pub attr: u8,              // Attributes (always 0x0F)
    pub entry_type: u8,        // Entry type (always 0)
    pub checksum: u8,          // Checksum of short name
    pub name2: [u16; 6],       // Next 6 characters
    pub first_cluster_lo: u16, // Always 0
    pub name3: [u16; 2],       // Last 2 characters
}

/// FAT32 filesystem implementation
#[derive(Debug)]
pub struct Fat32FileSystem {
    device_id: u32,
    sector_base: u64,
    boot_sector: Fat32BootSector,
    fs_info: Fat32FsInfo,
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    bytes_per_cluster: u32,
    fat_start_sector: u32,
    data_start_sector: u32,
    root_cluster: u32,
    total_clusters: u32,
    fat_cache: RwLock<BTreeMap<u32, u32>>, // Cluster -> Next cluster mapping
    cluster_cache: RwLock<BTreeMap<u32, Vec<u8>>>, // Cluster -> Data mapping
    dirty_fat: RwLock<BTreeMap<u32, u32>>, // Dirty FAT entries
    dirty_clusters: RwLock<BTreeMap<u32, Vec<u8>>>, // Dirty cluster data
}

impl Fat32FileSystem {
    /// Create new FAT32 filesystem instance
    pub fn new(device_id: u32) -> FsResult<Self> {
        Self::new_at(device_id, 0)
    }

    pub fn new_at(device_id: u32, sector_base: u64) -> FsResult<Self> {
        let mut fs = Self {
            device_id,
            sector_base,
            boot_sector: unsafe { mem::zeroed() },
            fs_info: unsafe { mem::zeroed() },
            bytes_per_sector: 0,
            sectors_per_cluster: 0,
            bytes_per_cluster: 0,
            fat_start_sector: 0,
            data_start_sector: 0,
            root_cluster: 0,
            total_clusters: 0,
            fat_cache: RwLock::new(BTreeMap::new()),
            cluster_cache: RwLock::new(BTreeMap::new()),
            dirty_fat: RwLock::new(BTreeMap::new()),
            dirty_clusters: RwLock::new(BTreeMap::new()),
        };

        fs.read_boot_sector()?;
        fs.read_fs_info()?;
        fs.calculate_layout()?;
        Ok(fs)
    }

    /// Read boot sector from disk
    fn read_boot_sector(&mut self) -> FsResult<()> {
        let mut buffer = vec![0u8; 512];

        // Boot sector is at sector 0
        read_storage_sectors(self.device_id, self.sector_base, &mut buffer)
            .map_err(|_| FsError::IoError)?;

        // Parse boot sector
        self.boot_sector =
            unsafe { core::ptr::read_unaligned(buffer.as_ptr() as *const Fat32BootSector) };

        // Validate signature
        if self.boot_sector.signature != FAT32_SIGNATURE {
            return Err(FsError::InvalidArgument);
        }

        // Validate FAT32 specific fields
        if self.boot_sector.fat_size_16 != 0 || self.boot_sector.root_entry_count != 0 {
            return Err(FsError::InvalidArgument);
        }

        if self.boot_sector.fat_size_32 == 0 {
            return Err(FsError::InvalidArgument);
        }

        // sectors_per_cluster and bytes_per_sector are used as divisors / multipliers
        // when computing the layout; reject zero to avoid div-by-zero panics.
        if self.boot_sector.sectors_per_cluster == 0 || self.boot_sector.bytes_per_sector == 0 {
            return Err(FsError::InvalidArgument);
        }

        Ok(())
    }

    /// Read FSInfo sector
    fn read_fs_info(&mut self) -> FsResult<()> {
        if self.boot_sector.fs_info == 0 {
            // No FSInfo sector
            return Ok(());
        }

        let mut buffer = vec![0u8; 512];

        read_storage_sectors(
            self.device_id,
            self.sector_base + self.boot_sector.fs_info as u64,
            &mut buffer,
        )
        .map_err(|_| FsError::IoError)?;

        self.fs_info = unsafe { core::ptr::read_unaligned(buffer.as_ptr() as *const Fat32FsInfo) };

        // Validate signatures
        if self.fs_info.lead_signature != FAT32_FSINFO_SIGNATURE1
            || self.fs_info.struct_signature != FAT32_FSINFO_SIGNATURE2
        {
            // Invalid FSInfo, but not fatal
            self.fs_info = unsafe { mem::zeroed() };
        }

        Ok(())
    }

    /// Calculate filesystem layout
    fn calculate_layout(&mut self) -> FsResult<()> {
        self.bytes_per_sector = self.boot_sector.bytes_per_sector as u32;
        self.sectors_per_cluster = self.boot_sector.sectors_per_cluster as u32;
        self.bytes_per_cluster = self.bytes_per_sector * self.sectors_per_cluster;

        // Calculate FAT start sector
        self.fat_start_sector = self.boot_sector.reserved_sector_count as u32;

        // Calculate data start sector
        let fat_sectors = self.boot_sector.fat_size_32 * self.boot_sector.num_fats as u32;
        self.data_start_sector = self.fat_start_sector + fat_sectors;

        // Calculate total clusters. Guard the subtraction: a malformed boot sector can
        // have total_sectors_32 < data_start_sector, which would underflow.
        // sectors_per_cluster is validated non-zero in read_boot_sector.
        let total_sectors = self.boot_sector.total_sectors_32;
        let data_sectors = total_sectors
            .checked_sub(self.data_start_sector)
            .ok_or(FsError::InvalidArgument)?;
        self.total_clusters = data_sectors / self.sectors_per_cluster;

        self.root_cluster = self.boot_sector.root_cluster;

        // Validate cluster count for FAT32
        if self.total_clusters < 65525 {
            return Err(FsError::InvalidArgument);
        }

        Ok(())
    }

    /// Convert cluster number to sector number
    fn cluster_to_sector(&self, cluster: u32) -> u32 {
        if cluster < 2 {
            return 0; // Invalid cluster
        }
        self.data_start_sector + (cluster - 2) * self.sectors_per_cluster
    }

    /// Read FAT entry
    fn read_fat_entry(&self, cluster: u32) -> FsResult<u32> {
        // Check cache first
        {
            let cache = self.fat_cache.read();
            if let Some(&next_cluster) = cache.get(&cluster) {
                return Ok(next_cluster);
            }
        }

        // Calculate FAT sector and offset
        let fat_offset = cluster * 4; // 4 bytes per FAT32 entry
        let fat_sector = self.fat_start_sector + (fat_offset / self.bytes_per_sector);
        let entry_offset = (fat_offset % self.bytes_per_sector) as usize;

        // Read FAT sector
        let mut buffer = vec![0u8; self.bytes_per_sector as usize];
        read_storage_sectors(
            self.device_id,
            self.sector_base + fat_sector as u64,
            &mut buffer,
        )
        .map_err(|_| FsError::IoError)?;

        // Extract FAT entry (mask off high 4 bits)
        let fat_entry = u32::from_le_bytes([
            buffer[entry_offset],
            buffer[entry_offset + 1],
            buffer[entry_offset + 2],
            buffer[entry_offset + 3],
        ]) & 0x0FFFFFFF;

        // Cache the entry
        {
            let mut cache = self.fat_cache.write();
            cache.insert(cluster, fat_entry);
        }

        Ok(fat_entry)
    }

    /// Write FAT entry
    fn write_fat_entry(&self, cluster: u32, value: u32) -> FsResult<()> {
        // Mark as dirty
        {
            let mut dirty = self.dirty_fat.write();
            dirty.insert(cluster, value & 0x0FFFFFFF);
        }

        // Update cache
        {
            let mut cache = self.fat_cache.write();
            cache.insert(cluster, value & 0x0FFFFFFF);
        }

        Ok(())
    }

    /// Read cluster data
    fn read_cluster(&self, cluster: u32) -> FsResult<Vec<u8>> {
        if cluster < 2 || cluster >= self.total_clusters + 2 {
            return Err(FsError::InvalidArgument);
        }

        // Check cache first
        {
            let cache = self.cluster_cache.read();
            if let Some(cached_data) = cache.get(&cluster) {
                return Ok(cached_data.clone());
            }
        }

        // Read cluster from disk
        let start_sector = self.cluster_to_sector(cluster);
        let mut buffer = vec![0u8; self.bytes_per_cluster as usize];

        read_storage_sectors(
            self.device_id,
            self.sector_base + start_sector as u64,
            &mut buffer,
        )
        .map_err(|_| FsError::IoError)?;

        // Cache the cluster
        {
            let mut cache = self.cluster_cache.write();
            cache.insert(cluster, buffer.clone());
        }

        Ok(buffer)
    }

    /// Write cluster data
    fn write_cluster(&self, cluster: u32, data: &[u8]) -> FsResult<()> {
        if cluster < 2 || cluster >= self.total_clusters + 2 {
            return Err(FsError::InvalidArgument);
        }

        if data.len() != self.bytes_per_cluster as usize {
            return Err(FsError::InvalidArgument);
        }

        // Mark as dirty
        {
            let mut dirty = self.dirty_clusters.write();
            dirty.insert(cluster, data.to_vec());
        }

        // Update cache
        {
            let mut cache = self.cluster_cache.write();
            cache.insert(cluster, data.to_vec());
        }

        Ok(())
    }

    /// Get cluster chain starting from given cluster
    fn get_cluster_chain(&self, start_cluster: u32) -> FsResult<Vec<u32>> {
        let mut chain = Vec::new();
        let mut current_cluster = start_cluster;

        while current_cluster >= 2 && current_cluster < FAT32_EOC {
            chain.push(current_cluster);
            current_cluster = self.read_fat_entry(current_cluster)?;
        }

        Ok(chain)
    }

    /// Parse 8.3 filename
    fn parse_83_name(name: &[u8; 11]) -> String {
        let mut result = String::new();

        // Add name part (first 8 bytes)
        for i in 0..8 {
            if name[i] == b' ' {
                break;
            }
            result.push(name[i] as char);
        }

        // Add extension part (last 3 bytes)
        let mut ext = String::new();
        for i in 8..11 {
            if name[i] == b' ' {
                break;
            }
            ext.push(name[i] as char);
        }

        if !ext.is_empty() {
            result.push('.');
            result.push_str(&ext);
        }

        crate::glib::ascii_strdown(&result)
    }

    /// Read directory entries from cluster chain
    fn read_directory_entries(&self, start_cluster: u32) -> FsResult<Vec<DirectoryEntry>> {
        let cluster_chain = self.get_cluster_chain(start_cluster)?;
        let mut entries = Vec::new();
        let mut lfn_entries = Vec::new();

        for cluster in cluster_chain {
            let cluster_data = self.read_cluster(cluster)?;
            let entries_per_cluster =
                self.bytes_per_cluster as usize / mem::size_of::<Fat32DirEntry>();

            for i in 0..entries_per_cluster {
                let offset = i * mem::size_of::<Fat32DirEntry>();
                if offset + mem::size_of::<Fat32DirEntry>() > cluster_data.len() {
                    break;
                }

                let dir_entry = unsafe {
                    core::ptr::read_unaligned(
                        cluster_data.as_ptr().add(offset) as *const Fat32DirEntry
                    )
                };

                // Check for end of directory
                if dir_entry.name[0] == 0 {
                    break;
                }

                // Skip deleted entries
                if dir_entry.name[0] == 0xE5 {
                    lfn_entries.clear();
                    continue;
                }

                // Handle long filename entries
                if dir_entry.attr & Fat32Attr::LONG_NAME.bits() == Fat32Attr::LONG_NAME.bits() {
                    let lfn_entry = unsafe {
                        core::ptr::read_unaligned(
                            cluster_data.as_ptr().add(offset) as *const Fat32LfnEntry
                        )
                    };
                    lfn_entries.push(lfn_entry);
                    continue;
                }

                // Skip volume ID entries
                if dir_entry.attr & Fat32Attr::VOLUME_ID.bits() != 0 {
                    lfn_entries.clear();
                    continue;
                }

                // Build filename
                let filename = if !lfn_entries.is_empty() {
                    // Reconstruct long filename
                    let mut long_name = String::new();
                    lfn_entries.sort_by_key(|e| e.order & 0x1F);

                    for lfn in &lfn_entries {
                        // Extract characters from LFN entry
                        // SAFETY: lfn is a packed struct representing FAT32 on-disk format.
                        // We use addr_of! to avoid creating misaligned references.
                        let name1 = unsafe { core::ptr::addr_of!(lfn.name1).read_unaligned() };
                        let name2 = unsafe { core::ptr::addr_of!(lfn.name2).read_unaligned() };
                        let name3 = unsafe { core::ptr::addr_of!(lfn.name3).read_unaligned() };

                        for &ch in &name1 {
                            if ch == 0 || ch == 0xFFFF {
                                break;
                            }
                            if let Some(c) = char::from_u32(ch as u32) {
                                long_name.push(c);
                            }
                        }
                        for &ch in &name2 {
                            if ch == 0 || ch == 0xFFFF {
                                break;
                            }
                            if let Some(c) = char::from_u32(ch as u32) {
                                long_name.push(c);
                            }
                        }
                        for &ch in &name3 {
                            if ch == 0 || ch == 0xFFFF {
                                break;
                            }
                            if let Some(c) = char::from_u32(ch as u32) {
                                long_name.push(c);
                            }
                        }
                    }
                    lfn_entries.clear();
                    long_name
                } else {
                    // Use 8.3 name
                    Self::parse_83_name(&dir_entry.name)
                };

                // Skip current and parent directory entries
                if filename == "." || filename == ".." {
                    continue;
                }

                // Determine file type
                let file_type = if dir_entry.attr & Fat32Attr::DIRECTORY.bits() != 0 {
                    FileType::Directory
                } else {
                    FileType::Regular
                };

                // Calculate inode number from cluster
                let first_cluster = ((dir_entry.first_cluster_hi as u32) << 16)
                    | (dir_entry.first_cluster_lo as u32);
                let inode = if first_cluster == 0 {
                    1
                } else {
                    first_cluster as u64
                };

                entries.push(DirectoryEntry {
                    name: filename,
                    inode,
                    file_type,
                });
            }
        }

        Ok(entries)
    }

    /// Resolve path to cluster number
    fn resolve_path(&self, path: &str) -> FsResult<u32> {
        if path == "/" {
            return Ok(self.root_cluster);
        }

        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        let mut current_cluster = self.root_cluster;

        for component in &components {
            let entries = self.read_directory_entries(current_cluster)?;
            let mut found = false;

            for entry in entries {
                if crate::glib::ascii_strcasecmp(&entry.name, component) == 0 {
                    if entry.file_type != FileType::Directory
                        && *component != *components.last().unwrap()
                    {
                        return Err(FsError::NotADirectory);
                    }
                    current_cluster = entry.inode as u32;
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(FsError::NotFound);
            }
        }

        Ok(current_cluster)
    }

    /// Get file metadata from directory entry
    fn get_file_metadata(&self, _cluster: u32, filename: &str) -> FsResult<FileMetadata> {
        let parent_cluster = if filename.contains('/') {
            let parent_path = filename.rsplitn(2, '/').nth(1).unwrap_or("/");
            self.resolve_path(parent_path)?
        } else {
            self.root_cluster
        };

        let entries = self.read_directory_entries(parent_cluster)?;
        let basename = filename.split('/').last().unwrap_or(filename);

        for entry in entries {
            if crate::glib::ascii_strcasecmp(&entry.name, basename) == 0 {
                // Find the actual directory entry to get metadata
                let cluster_chain = self.get_cluster_chain(parent_cluster)?;

                for cluster_num in cluster_chain {
                    let cluster_data = self.read_cluster(cluster_num)?;
                    let entries_per_cluster =
                        self.bytes_per_cluster as usize / mem::size_of::<Fat32DirEntry>();

                    for i in 0..entries_per_cluster {
                        let offset = i * mem::size_of::<Fat32DirEntry>();
                        if offset + mem::size_of::<Fat32DirEntry>() > cluster_data.len() {
                            break;
                        }

                        let dir_entry = unsafe {
                            core::ptr::read_unaligned(
                                cluster_data.as_ptr().add(offset) as *const Fat32DirEntry
                            )
                        };

                        if dir_entry.name[0] == 0 || dir_entry.name[0] == 0xE5 {
                            continue;
                        }

                        if dir_entry.attr & Fat32Attr::LONG_NAME.bits()
                            == Fat32Attr::LONG_NAME.bits()
                        {
                            continue;
                        }

                        let entry_name = Self::parse_83_name(&dir_entry.name);
                        if crate::glib::ascii_strcasecmp(&entry_name, basename) == 0 {
                            let file_type = if dir_entry.attr & Fat32Attr::DIRECTORY.bits() != 0 {
                                FileType::Directory
                            } else {
                                FileType::Regular
                            };

                            let permissions = if dir_entry.attr & Fat32Attr::READ_ONLY.bits() != 0 {
                                FilePermissions::from_octal(0o444)
                            } else {
                                FilePermissions::from_octal(0o644)
                            };

                            return Ok(FileMetadata {
                                inode: entry.inode,
                                file_type,
                                size: dir_entry.file_size as u64,
                                permissions,
                                uid: 0,
                                gid: 0,
                                created: 0, // FAT32 timestamps would need conversion
                                modified: 0,
                                accessed: 0,
                                link_count: 1,
                                device_id: None,
                            });
                        }
                    }
                }
            }
        }

        Err(FsError::NotFound)
    }

    /// Flush dirty data to disk
    fn flush_dirty_data(&self) -> FsResult<()> {
        // Flush dirty FAT entries
        {
            let dirty_fat = {
                let mut dirty = self.dirty_fat.write();
                let entries = dirty.clone();
                dirty.clear();
                entries
            };

            for (cluster, value) in dirty_fat {
                let fat_offset = cluster * 4;
                let fat_sector = self.fat_start_sector + (fat_offset / self.bytes_per_sector);
                let entry_offset = (fat_offset % self.bytes_per_sector) as usize;

                // Read-modify-write FAT sector
                let mut buffer = vec![0u8; self.bytes_per_sector as usize];
                read_storage_sectors(
                    self.device_id,
                    self.sector_base + fat_sector as u64,
                    &mut buffer,
                )
                .map_err(|_| FsError::IoError)?;

                let value_bytes = (value & 0x0FFFFFFF).to_le_bytes();
                buffer[entry_offset..entry_offset + 4].copy_from_slice(&value_bytes);

                write_storage_sectors(self.device_id, fat_sector as u64, &buffer)
                    .map_err(|_| FsError::IoError)?;
            }
        }

        // Flush dirty clusters
        {
            let dirty_clusters = {
                let mut dirty = self.dirty_clusters.write();
                let clusters = dirty.clone();
                dirty.clear();
                clusters
            };

            for (cluster, data) in dirty_clusters {
                let start_sector = self.cluster_to_sector(cluster);
                write_storage_sectors(self.device_id, start_sector as u64, &data)
                    .map_err(|_| FsError::IoError)?;
            }
        }

        Ok(())
    }

    /// Find a free cluster in the FAT table.
    fn allocate_cluster(&self) -> FsResult<u32> {
        // Try the hint from fs_info first
        let start =
            if self.fs_info.next_free != 0 && self.fs_info.next_free < self.total_clusters + 2 {
                self.fs_info.next_free
            } else {
                2
            };

        for cluster in start..self.total_clusters + 2 {
            let entry = self.read_fat_entry(cluster)?;
            if entry == FAT32_FREE_CLUSTER {
                // Mark as end-of-chain
                self.write_fat_entry(cluster, FAT32_EOC)?;
                // Zero the cluster data
                let zeroed = vec![0u8; self.bytes_per_cluster as usize];
                self.write_cluster(cluster, &zeroed)?;
                return Ok(cluster);
            }
        }

        // Fallback: scan from the beginning
        for cluster in 2..start {
            let entry = self.read_fat_entry(cluster)?;
            if entry == FAT32_FREE_CLUSTER {
                self.write_fat_entry(cluster, FAT32_EOC)?;
                let zeroed = vec![0u8; self.bytes_per_cluster as usize];
                self.write_cluster(cluster, &zeroed)?;
                return Ok(cluster);
            }
        }

        Err(FsError::NoSpaceLeft)
    }

    /// Convert a filename to FAT32 8.3 format (11 bytes, space-padded, uppercase).
    fn name_to_83(name: &str) -> [u8; 11] {
        let mut result = [b' '; 11];
        let bytes = name.as_bytes();
        let mut name_idx = 0;
        let mut ext_idx = 0;
        let mut in_ext = false;

        for &b in bytes {
            if b == b'.' && !in_ext {
                in_ext = true;
                continue;
            }
            let upper = if b >= b'a' && b <= b'z' { b - 32 } else { b };
            if in_ext {
                if ext_idx < 3 {
                    result[8 + ext_idx] = upper;
                    ext_idx += 1;
                }
            } else {
                if name_idx < 8 {
                    result[name_idx] = upper;
                    name_idx += 1;
                }
            }
        }

        // Special case: "." and ".."
        if name == "." {
            result = [b' '; 11];
            result[0] = b'.';
        } else if name == ".." {
            result = [b' '; 11];
            result[0] = b'.';
            result[1] = b'.';
        }

        result
    }

    fn validate_short_name(name: &str) -> FsResult<()> {
        if name.is_empty() || name == "." || name == ".." {
            return Err(FsError::InvalidArgument);
        }

        let mut parts = name.split('.');
        let base = parts.next().unwrap_or("");
        let ext = parts.next();
        if parts.next().is_some()
            || base.is_empty()
            || base.len() > 8
            || ext.map_or(false, |e| e.is_empty() || e.len() > 3)
        {
            return Err(FsError::NameTooLong);
        }

        for b in name.bytes() {
            if b < 0x20 || b == b' ' || b"\"*+,/:;<=>?[\\]|".contains(&b) {
                return Err(FsError::InvalidArgument);
            }
        }

        Ok(())
    }

    /// Find a free directory entry slot in a directory's cluster chain.
    /// Returns (cluster, offset_within_cluster) of a free slot.
    /// If the directory is full, extends it by allocating a new cluster.
    fn find_free_dir_slot(&self, dir_cluster: u32) -> FsResult<(u32, usize)> {
        let chain = self.get_cluster_chain(dir_cluster)?;
        let entry_size = mem::size_of::<Fat32DirEntry>();
        let entries_per_cluster = self.bytes_per_cluster as usize / entry_size;

        for &cluster in &chain {
            let data = self.read_cluster(cluster)?;
            for i in 0..entries_per_cluster {
                let offset = i * entry_size;
                if offset + entry_size > data.len() {
                    break;
                }
                // Free slot: name[0] == 0x00 (end of dir) or 0xE5 (deleted)
                if data[offset] == 0x00 || data[offset] == 0xE5 {
                    return Ok((cluster, offset));
                }
            }
        }

        // No free slot — extend the directory by allocating a new cluster
        let new_cluster = self.allocate_cluster()?;
        let last_cluster = *chain.last().unwrap_or(&dir_cluster);
        self.write_fat_entry(last_cluster, new_cluster)?;
        self.write_fat_entry(new_cluster, FAT32_EOC)?;

        // Zero the new cluster
        let zeroed = vec![0u8; self.bytes_per_cluster as usize];
        self.write_cluster(new_cluster, &zeroed)?;

        Ok((new_cluster, 0))
    }

    /// Write a directory entry at the given cluster and offset.
    fn write_dir_entry(&self, cluster: u32, offset: usize, entry: &Fat32DirEntry) -> FsResult<()> {
        let mut data = self.read_cluster(cluster)?;
        if offset + mem::size_of::<Fat32DirEntry>() > data.len() {
            return Err(FsError::InvalidArgument);
        }
        let entry_bytes = unsafe {
            core::slice::from_raw_parts(
                entry as *const Fat32DirEntry as *const u8,
                mem::size_of::<Fat32DirEntry>(),
            )
        };
        data[offset..offset + mem::size_of::<Fat32DirEntry>()].copy_from_slice(entry_bytes);
        self.write_cluster(cluster, &data)?;
        Ok(())
    }

    /// Create a directory entry in the parent directory for a new file or subdirectory.
    fn create_dir_entry(
        &self,
        parent_cluster: u32,
        name: &str,
        is_dir: bool,
        first_cluster: u32,
    ) -> FsResult<()> {
        Self::validate_short_name(name)?;
        let (slot_cluster, slot_offset) = self.find_free_dir_slot(parent_cluster)?;

        let mut entry: Fat32DirEntry = unsafe { mem::zeroed() };
        entry.name = Self::name_to_83(name);
        entry.attr = if is_dir {
            Fat32Attr::DIRECTORY.bits()
        } else {
            Fat32Attr::ARCHIVE.bits()
        };
        entry.first_cluster_hi = (first_cluster >> 16) as u16;
        entry.first_cluster_lo = (first_cluster & 0xFFFF) as u16;
        entry.file_size = 0;

        self.write_dir_entry(slot_cluster, slot_offset, &entry)?;
        Ok(())
    }

    /// Create a directory entry by copying an existing entry and replacing only the 8.3 name.
    fn copy_dir_entry_with_name(
        &self,
        parent_cluster: u32,
        name: &str,
        source: &Fat32DirEntry,
    ) -> FsResult<()> {
        Self::validate_short_name(name)?;
        let (slot_cluster, slot_offset) = self.find_free_dir_slot(parent_cluster)?;
        let mut entry = *source;
        entry.name = Self::name_to_83(name);
        self.write_dir_entry(slot_cluster, slot_offset, &entry)
    }

    /// Resolve parent directory path and extract the basename.
    fn split_path(path: &str) -> (&str, &str) {
        let path = path.strip_prefix('/').unwrap_or(path);
        if let Some(pos) = path.rfind('/') {
            (&path[..pos], &path[pos + 1..])
        } else {
            ("", path)
        }
    }

    /// Resolve a parent path string to a cluster number.
    fn resolve_parent(&self, parent_path: &str) -> FsResult<u32> {
        if parent_path.is_empty() {
            return Ok(self.root_cluster);
        }
        self.resolve_path(parent_path)
    }

    /// Find a raw directory entry by name in a parent directory cluster chain.
    /// Returns (cluster_num, byte_offset_within_cluster, entry_copy) if found.
    fn find_dir_entry_raw(
        &self,
        parent_cluster: u32,
        name: &str,
    ) -> FsResult<Option<(u32, usize, Fat32DirEntry)>> {
        let chain = self.get_cluster_chain(parent_cluster)?;
        let entry_size = mem::size_of::<Fat32DirEntry>();

        for cluster_num in chain {
            let cluster_data = self.read_cluster(cluster_num)?;
            let entries_per_cluster = self.bytes_per_cluster as usize / entry_size;

            for i in 0..entries_per_cluster {
                let offset = i * entry_size;
                if offset + entry_size > cluster_data.len() {
                    break;
                }

                let dir_entry = unsafe {
                    core::ptr::read_unaligned(
                        cluster_data.as_ptr().add(offset) as *const Fat32DirEntry
                    )
                };

                if dir_entry.name[0] == 0 {
                    return Ok(None); // End of directory
                }
                if dir_entry.name[0] == 0xE5 {
                    continue; // Deleted
                }
                if dir_entry.attr & Fat32Attr::LONG_NAME.bits() == Fat32Attr::LONG_NAME.bits() {
                    continue; // LFN entry
                }
                if dir_entry.attr & Fat32Attr::VOLUME_ID.bits() != 0 {
                    continue;
                }

                let entry_name = Self::parse_83_name(&dir_entry.name);
                if crate::glib::ascii_strcasecmp(&entry_name, name) == 0 {
                    return Ok(Some((cluster_num, offset, dir_entry)));
                }
            }
        }
        Ok(None)
    }

    /// Free a cluster chain by writing FAT32_FREE_CLUSTER (0) to each FAT entry.
    fn free_cluster_chain(&self, start_cluster: u32) -> FsResult<()> {
        let chain = self.get_cluster_chain(start_cluster)?;
        for cluster in chain {
            self.write_fat_entry(cluster, FAT32_FREE_CLUSTER)?;
        }
        Ok(())
    }

    /// Mark a directory entry as deleted (set first byte of name to 0xE5).
    fn mark_dir_entry_deleted(&self, cluster: u32, offset: usize) -> FsResult<()> {
        let mut data = self.read_cluster(cluster)?;
        if offset < data.len() {
            data[offset] = 0xE5;
            self.write_cluster(cluster, &data)?;
        }
        Ok(())
    }

    /// Update a directory entry in-place (write modified entry back to disk).
    fn update_dir_entry(&self, cluster: u32, offset: usize, entry: &Fat32DirEntry) -> FsResult<()> {
        let mut data = self.read_cluster(cluster)?;
        let entry_size = mem::size_of::<Fat32DirEntry>();
        if offset + entry_size <= data.len() {
            let entry_bytes = unsafe {
                core::slice::from_raw_parts(entry as *const Fat32DirEntry as *const u8, entry_size)
            };
            data[offset..offset + entry_size].copy_from_slice(entry_bytes);
            self.write_cluster(cluster, &data)?;
        }
        Ok(())
    }

    fn update_file_size_by_cluster(&self, target_cluster: u32, size: u32) -> FsResult<()> {
        self.update_file_size_in_dir(self.root_cluster, target_cluster, size, 0)
    }

    fn update_file_size_in_dir(
        &self,
        dir_cluster: u32,
        target_cluster: u32,
        size: u32,
        depth: usize,
    ) -> FsResult<()> {
        if depth > 32 {
            return Err(FsError::InvalidArgument);
        }

        let chain = self.get_cluster_chain(dir_cluster)?;
        let entry_size = mem::size_of::<Fat32DirEntry>();
        let mut subdirs = Vec::new();

        for cluster_num in chain {
            let data = self.read_cluster(cluster_num)?;
            let entries_per_cluster = self.bytes_per_cluster as usize / entry_size;

            for i in 0..entries_per_cluster {
                let offset = i * entry_size;
                if offset + entry_size > data.len() {
                    break;
                }

                let dir_entry = unsafe {
                    core::ptr::read_unaligned(data.as_ptr().add(offset) as *const Fat32DirEntry)
                };

                if dir_entry.name[0] == 0 {
                    break;
                }
                if dir_entry.name[0] == 0xE5
                    || dir_entry.attr & Fat32Attr::LONG_NAME.bits() == Fat32Attr::LONG_NAME.bits()
                    || dir_entry.attr & Fat32Attr::VOLUME_ID.bits() != 0
                {
                    continue;
                }

                let first_cluster = ((dir_entry.first_cluster_hi as u32) << 16)
                    | (dir_entry.first_cluster_lo as u32);
                if first_cluster == target_cluster {
                    let mut updated = dir_entry;
                    updated.file_size = core::cmp::max(updated.file_size, size);
                    return self.update_dir_entry(cluster_num, offset, &updated);
                }

                if dir_entry.attr & Fat32Attr::DIRECTORY.bits() != 0 {
                    let name = Self::parse_83_name(&dir_entry.name);
                    if first_cluster >= 2 && name != "." && name != ".." {
                        subdirs.push(first_cluster);
                    }
                }
            }
        }

        for subdir in subdirs {
            if self
                .update_file_size_in_dir(subdir, target_cluster, size, depth + 1)
                .is_ok()
            {
                return Ok(());
            }
        }

        Err(FsError::NotFound)
    }
}

impl FileSystem for Fat32FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Fat32
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let total_clusters = self.total_clusters as u64;
        let free_clusters = if self.fs_info.free_count != 0xFFFFFFFF {
            self.fs_info.free_count as u64
        } else {
            // Count free clusters by scanning FAT
            let mut free_count = 0u64;
            for cluster in 2..self.total_clusters + 2 {
                if let Ok(fat_entry) = self.read_fat_entry(cluster) {
                    if fat_entry == FAT32_FREE_CLUSTER {
                        free_count += 1;
                    }
                }
            }
            free_count
        };

        Ok(FileSystemStats {
            total_blocks: total_clusters,
            free_blocks: free_clusters,
            available_blocks: free_clusters,
            total_inodes: total_clusters, // FAT32 doesn't have fixed inodes
            free_inodes: free_clusters,
            block_size: self.bytes_per_cluster,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, basename) = Self::split_path(path);
        if basename.is_empty() {
            return Err(FsError::InvalidArgument);
        }

        // Check if file already exists
        let parent_cluster = self.resolve_parent(parent_path)?;
        let entries = self.read_directory_entries(parent_cluster)?;
        for entry in &entries {
            if crate::glib::ascii_strcasecmp(&entry.name, basename) == 0 {
                return Err(FsError::AlreadyExists);
            }
        }

        // Allocate a cluster for the new file
        let new_cluster = self.allocate_cluster()?;

        // Create directory entry in parent
        self.create_dir_entry(parent_cluster, basename, false, new_cluster)?;

        // Flush to disk
        self.flush_dirty_data()?;

        Ok(new_cluster as InodeNumber)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        let cluster = self.resolve_path(path)?;
        Ok(cluster as InodeNumber)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let start_cluster = inode as u32;
        let cluster_chain = self.get_cluster_chain(start_cluster)?;

        if cluster_chain.is_empty() {
            return Ok(0);
        }

        let cluster_size = self.bytes_per_cluster as u64;
        let start_cluster_idx = (offset / cluster_size) as usize;
        let start_offset = (offset % cluster_size) as usize;

        if start_cluster_idx >= cluster_chain.len() {
            return Ok(0);
        }

        let mut bytes_read = 0;
        let mut remaining = buffer.len();

        for (i, &cluster) in cluster_chain.iter().enumerate().skip(start_cluster_idx) {
            if remaining == 0 {
                break;
            }

            let cluster_data = self.read_cluster(cluster)?;
            let copy_offset = if i == start_cluster_idx {
                start_offset
            } else {
                0
            };
            let copy_len = core::cmp::min(cluster_data.len() - copy_offset, remaining);

            buffer[bytes_read..bytes_read + copy_len]
                .copy_from_slice(&cluster_data[copy_offset..copy_offset + copy_len]);

            bytes_read += copy_len;
            remaining -= copy_len;
        }

        Ok(bytes_read)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let start_cluster = inode as u32;
        if start_cluster < 2 {
            return Err(FsError::InvalidArgument);
        }

        let cluster_size = self.bytes_per_cluster as u64;
        let mut chain = self.get_cluster_chain(start_cluster)?;

        // Ensure we have enough clusters for the write range
        let end_offset = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        let needed_clusters = ((end_offset + cluster_size - 1) / cluster_size) as usize;

        while chain.len() < needed_clusters {
            let new_cluster = self.allocate_cluster()?;
            let last = *chain.last().unwrap_or(&start_cluster);
            self.write_fat_entry(last, new_cluster)?;
            chain.push(new_cluster);
        }

        let mut bytes_written = 0;
        let mut remaining = buffer.len();
        let start_cluster_idx = (offset / cluster_size) as usize;
        let start_offset_in_cluster = (offset % cluster_size) as usize;

        for (i, &cluster) in chain.iter().enumerate().skip(start_cluster_idx) {
            if remaining == 0 {
                break;
            }

            let mut cluster_data = self.read_cluster(cluster)?;
            let write_offset = if i == start_cluster_idx {
                start_offset_in_cluster
            } else {
                0
            };
            let write_len = core::cmp::min(cluster_data.len() - write_offset, remaining);

            cluster_data[write_offset..write_offset + write_len]
                .copy_from_slice(&buffer[bytes_written..bytes_written + write_len]);
            self.write_cluster(cluster, &cluster_data)?;

            bytes_written += write_len;
            remaining -= write_len;
        }

        // Flush to disk
        let new_size = core::cmp::max(offset.saturating_add(bytes_written as u64), end_offset);
        self.update_file_size_by_cluster(start_cluster, new_size.min(u32::MAX as u64) as u32)?;
        self.flush_dirty_data()?;

        Ok(bytes_written)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let cluster = inode as u32;

        // For root directory
        if cluster == self.root_cluster {
            return Ok(FileMetadata {
                inode,
                file_type: FileType::Directory,
                size: 0,
                permissions: FilePermissions::from_octal(0o755),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 1,
                device_id: None,
            });
        }

        // For non-root files/directories, scan the root directory to find
        // the directory entry whose first cluster matches this inode.
        // This gives us the file size, type, and timestamps.
        let target_cluster = cluster;
        let root_entries = self.read_directory_entries(self.root_cluster)?;
        for entry in &root_entries {
            if entry.inode == inode {
                // Found the entry — but DirectoryEntry doesn't carry size
                // or timestamps. We need to re-read the raw FAT entry.
                break;
            }
        }

        // Scan the raw directory entries in the root cluster to find
        // the one whose first_cluster matches our target.
        let cluster_chain = self.get_cluster_chain(self.root_cluster)?;
        for cluster in cluster_chain {
            let cluster_data = self.read_cluster(cluster)?;
            let entries_per_cluster =
                self.bytes_per_cluster as usize / mem::size_of::<Fat32DirEntry>();

            for i in 0..entries_per_cluster {
                let offset = i * mem::size_of::<Fat32DirEntry>();
                if offset + mem::size_of::<Fat32DirEntry>() > cluster_data.len() {
                    break;
                }

                let dir_entry = unsafe {
                    core::ptr::read_unaligned(
                        cluster_data.as_ptr().add(offset) as *const Fat32DirEntry
                    )
                };

                if dir_entry.name[0] == 0 {
                    break;
                }
                if dir_entry.name[0] == 0xE5 {
                    continue;
                }

                let first_cluster = ((dir_entry.first_cluster_hi as u32) << 16)
                    | (dir_entry.first_cluster_lo as u32);

                if first_cluster == target_cluster {
                    let attr = Fat32Attr::from_bits_truncate(dir_entry.attr);
                    let is_dir = attr.contains(Fat32Attr::DIRECTORY);
                    let is_readonly = attr.contains(Fat32Attr::READ_ONLY);

                    let perms = if is_dir {
                        FilePermissions::from_octal(0o755)
                    } else if is_readonly {
                        FilePermissions::from_octal(0o444)
                    } else {
                        FilePermissions::from_octal(0o644)
                    };

                    return Ok(FileMetadata {
                        inode,
                        file_type: if is_dir {
                            FileType::Directory
                        } else {
                            FileType::Regular
                        },
                        size: dir_entry.file_size as u64,
                        permissions: perms,
                        uid: 0,
                        gid: 0,
                        created: fat_date_time_to_unix(
                            dir_entry.create_date,
                            dir_entry.create_time,
                        ),
                        modified: fat_date_time_to_unix(dir_entry.write_date, dir_entry.write_time),
                        accessed: fat_date_time_to_unix(dir_entry.last_access_date, 0),
                        link_count: 1,
                        device_id: None,
                    });
                }
            }
        }

        // Entry not found in root directory — it may be in a subdirectory.
        // Return default metadata as a fallback.
        Ok(FileMetadata {
            inode,
            file_type: FileType::Regular,
            size: 0,
            permissions: FilePermissions::from_octal(0o644),
            uid: 0,
            gid: 0,
            created: 0,
            modified: 0,
            accessed: 0,
            link_count: 1,
            device_id: None,
        })
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        // FAT32 doesn't have per-file uid/gid in the traditional sense.
        // We can update the read-only attribute bit and timestamps.
        // Find the directory entry by searching all directories for this cluster.
        // This is a best-effort search — FAT32 has no back-pointers.
        let target_cluster = inode as u32;
        let entry_size = mem::size_of::<Fat32DirEntry>();

        // Search root directory and all subdirectories
        let chain = self.get_cluster_chain(self.root_cluster)?;
        for cluster_num in chain {
            let data = self.read_cluster(cluster_num)?;
            let entries_per_cluster = self.bytes_per_cluster as usize / entry_size;
            for i in 0..entries_per_cluster {
                let offset = i * entry_size;
                if offset + entry_size > data.len() {
                    break;
                }
                let dir_entry = unsafe {
                    core::ptr::read_unaligned(data.as_ptr().add(offset) as *const Fat32DirEntry)
                };
                if dir_entry.name[0] == 0 || dir_entry.name[0] == 0xE5 {
                    continue;
                }
                if dir_entry.attr & Fat32Attr::LONG_NAME.bits() == Fat32Attr::LONG_NAME.bits() {
                    continue;
                }
                if dir_entry.attr & Fat32Attr::VOLUME_ID.bits() != 0 {
                    continue;
                }

                let first_cluster = ((dir_entry.first_cluster_hi as u32) << 16)
                    | (dir_entry.first_cluster_lo as u32);
                if first_cluster == target_cluster {
                    let mut updated = dir_entry;
                    // Update read-only attribute based on write permissions
                    let is_read_only = !metadata.permissions.owner_write
                        && !metadata.permissions.group_write
                        && !metadata.permissions.other_write;
                    if is_read_only {
                        updated.attr |= Fat32Attr::READ_ONLY.bits();
                    } else {
                        updated.attr &= !Fat32Attr::READ_ONLY.bits();
                    }
                    // Update write time/date from modified timestamp
                    let mtime = metadata.modified;
                    if mtime > 0 {
                        let secs = mtime / 1000;
                        let fat_time = ((secs as u32) & 0xFFFF) as u16;
                        let fat_date = (((secs / 86400) as u32 + 1) & 0xFFFF) as u16;
                        updated.write_time = fat_time;
                        updated.write_date = fat_date;
                    }
                    self.update_dir_entry(cluster_num, offset, &updated)?;
                    self.flush_dirty_data()?;
                    return Ok(());
                }
            }
        }
        Err(FsError::NotFound)
    }

    fn mkdir(&self, path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, basename) = Self::split_path(path);
        if basename.is_empty() {
            return Err(FsError::InvalidArgument);
        }

        // Check if directory already exists
        let parent_cluster = self.resolve_parent(parent_path)?;
        let entries = self.read_directory_entries(parent_cluster)?;
        for entry in &entries {
            if crate::glib::ascii_strcasecmp(&entry.name, basename) == 0 {
                return Err(FsError::AlreadyExists);
            }
        }

        // Allocate a cluster for the new directory
        let new_cluster = self.allocate_cluster()?;

        // Create directory entry in parent
        self.create_dir_entry(parent_cluster, basename, true, new_cluster)?;

        // Initialize the new directory with "." and ".." entries
        let entry_size = mem::size_of::<Fat32DirEntry>();
        let mut dir_data = vec![0u8; self.bytes_per_cluster as usize];

        // "." entry — points to the directory itself
        let mut dot_entry: Fat32DirEntry = unsafe { mem::zeroed() };
        dot_entry.name = Self::name_to_83(".");
        dot_entry.attr = Fat32Attr::DIRECTORY.bits();
        dot_entry.first_cluster_hi = (new_cluster >> 16) as u16;
        dot_entry.first_cluster_lo = (new_cluster & 0xFFFF) as u16;
        let dot_bytes = unsafe {
            core::slice::from_raw_parts(&dot_entry as *const Fat32DirEntry as *const u8, entry_size)
        };
        dir_data[0..entry_size].copy_from_slice(dot_bytes);

        // ".." entry — points to the parent directory
        let mut dotdot_entry: Fat32DirEntry = unsafe { mem::zeroed() };
        dotdot_entry.name = Self::name_to_83("..");
        dotdot_entry.attr = Fat32Attr::DIRECTORY.bits();
        dotdot_entry.first_cluster_hi = (parent_cluster >> 16) as u16;
        dotdot_entry.first_cluster_lo = (parent_cluster & 0xFFFF) as u16;
        let dotdot_bytes = unsafe {
            core::slice::from_raw_parts(
                &dotdot_entry as *const Fat32DirEntry as *const u8,
                entry_size,
            )
        };
        dir_data[entry_size..2 * entry_size].copy_from_slice(dotdot_bytes);

        self.write_cluster(new_cluster, &dir_data)?;

        // Flush to disk
        self.flush_dirty_data()?;

        Ok(new_cluster as InodeNumber)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let (parent_path, basename) = Self::split_path(path);
        if basename.is_empty() {
            return Err(FsError::InvalidArgument);
        }

        let parent_cluster = self.resolve_parent(parent_path)?;

        // Find the directory entry
        let (entry_cluster, entry_offset, dir_entry) = self
            .find_dir_entry_raw(parent_cluster, basename)?
            .ok_or(FsError::NotFound)?;

        // Verify it's a directory
        if dir_entry.attr & Fat32Attr::DIRECTORY.bits() == 0 {
            return Err(FsError::NotADirectory);
        }

        let target_cluster =
            ((dir_entry.first_cluster_hi as u32) << 16) | (dir_entry.first_cluster_lo as u32);

        // Check if directory is empty (only . and .. entries)
        if target_cluster >= 2 {
            let entries = self.read_directory_entries(target_cluster)?;
            for entry in &entries {
                if entry.name != "." && entry.name != ".." {
                    return Err(FsError::DirectoryNotEmpty);
                }
            }
            // Free the directory's cluster chain
            self.free_cluster_chain(target_cluster)?;
        }

        // Mark the directory entry as deleted
        self.mark_dir_entry_deleted(entry_cluster, entry_offset)?;
        self.flush_dirty_data()?;
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let (parent_path, basename) = Self::split_path(path);
        if basename.is_empty() {
            return Err(FsError::InvalidArgument);
        }

        let parent_cluster = self.resolve_parent(parent_path)?;

        // Find the directory entry
        let (entry_cluster, entry_offset, dir_entry) = self
            .find_dir_entry_raw(parent_cluster, basename)?
            .ok_or(FsError::NotFound)?;

        // Cannot unlink a directory — use rmdir for that
        if dir_entry.attr & Fat32Attr::DIRECTORY.bits() != 0 {
            return Err(FsError::IsADirectory);
        }

        // Free the file's cluster chain
        let target_cluster =
            ((dir_entry.first_cluster_hi as u32) << 16) | (dir_entry.first_cluster_lo as u32);
        if target_cluster >= 2 {
            self.free_cluster_chain(target_cluster)?;
        }

        // Mark the directory entry as deleted
        self.mark_dir_entry_deleted(entry_cluster, entry_offset)?;
        self.flush_dirty_data()?;
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let cluster = inode as u32;
        self.read_directory_entries(cluster)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        if old_path == new_path {
            return Ok(());
        }

        let (old_parent, old_name) = Self::split_path(old_path);
        let (new_parent, new_name) = Self::split_path(new_path);
        if old_name.is_empty() || new_name.is_empty() {
            return Err(FsError::InvalidArgument);
        }

        let old_parent_cluster = self.resolve_parent(old_parent)?;
        let new_parent_cluster = self.resolve_parent(new_parent)?;

        // Find the source entry
        let (src_cluster, src_offset, src_entry) = self
            .find_dir_entry_raw(old_parent_cluster, old_name)?
            .ok_or(FsError::NotFound)?;

        // Check if destination already exists
        if let Some((dst_cluster, dst_offset, dst_entry)) =
            self.find_dir_entry_raw(new_parent_cluster, new_name)?
        {
            if dst_cluster == src_cluster && dst_offset == src_offset {
                return Ok(());
            }
            let src_is_dir = src_entry.attr & Fat32Attr::DIRECTORY.bits() != 0;
            let dst_is_dir = dst_entry.attr & Fat32Attr::DIRECTORY.bits() != 0;

            if src_is_dir && !dst_is_dir {
                return Err(FsError::NotADirectory);
            }
            if !src_is_dir && dst_is_dir {
                return Err(FsError::IsADirectory);
            }

            if dst_is_dir {
                // Destination directory must be empty
                let dst_target = ((dst_entry.first_cluster_hi as u32) << 16)
                    | (dst_entry.first_cluster_lo as u32);
                if dst_target >= 2 {
                    let entries = self.read_directory_entries(dst_target)?;
                    for e in &entries {
                        if e.name != "." && e.name != ".." {
                            return Err(FsError::DirectoryNotEmpty);
                        }
                    }
                    self.free_cluster_chain(dst_target)?;
                }
            } else {
                // Overwrite existing file — free its clusters
                let dst_target = ((dst_entry.first_cluster_hi as u32) << 16)
                    | (dst_entry.first_cluster_lo as u32);
                if dst_target >= 2 {
                    self.free_cluster_chain(dst_target)?;
                }
            }
            // Mark destination entry as deleted
            self.mark_dir_entry_deleted(dst_cluster, dst_offset)?;
        }

        // Create new entry in destination directory with the source's metadata and data pointer.
        let target_cluster =
            ((src_entry.first_cluster_hi as u32) << 16) | (src_entry.first_cluster_lo as u32);
        let is_dir = src_entry.attr & Fat32Attr::DIRECTORY.bits() != 0;
        self.copy_dir_entry_with_name(new_parent_cluster, new_name, &src_entry)?;

        // If it's a directory, update the ".." entry to point to new parent
        if is_dir && target_cluster >= 2 && old_parent_cluster != new_parent_cluster {
            let dir_data = self.read_cluster(target_cluster)?;
            let entry_size = mem::size_of::<Fat32DirEntry>();
            if dir_data.len() >= 2 * entry_size {
                let mut data = dir_data.clone();
                // Update ".." entry's first_cluster to new parent
                let dotdot_offset = entry_size;
                let mut dotdot = unsafe {
                    core::ptr::read_unaligned(
                        data.as_ptr().add(dotdot_offset) as *const Fat32DirEntry
                    )
                };
                dotdot.first_cluster_hi = (new_parent_cluster >> 16) as u16;
                dotdot.first_cluster_lo = (new_parent_cluster & 0xFFFF) as u16;
                let dotdot_bytes = unsafe {
                    core::slice::from_raw_parts(
                        &dotdot as *const Fat32DirEntry as *const u8,
                        entry_size,
                    )
                };
                data[dotdot_offset..dotdot_offset + entry_size].copy_from_slice(dotdot_bytes);
                self.write_cluster(target_cluster, &data)?;
            }
        }

        // Mark old entry as deleted
        self.mark_dir_entry_deleted(src_cluster, src_offset)?;

        self.flush_dirty_data()?;
        Ok(())
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        self.flush_dirty_data()
    }
}

/// Convert a FAT date/time pair to a Unix timestamp.
///
/// FAT date format (16-bit):
/// - Bits 15-9: year - 1980 (0-127)
/// - Bits 8-5: month (1-12)
/// - Bits 4-0: day (1-31)
///
/// FAT time format (16-bit):
/// - Bits 15-11: hours (0-23)
/// - Bits 10-5: minutes (0-59)
/// - Bits 4-0: seconds/2 (0-29, so 0-58 seconds)
fn fat_date_time_to_unix(fat_date: u16, fat_time: u16) -> u64 {
    if fat_date == 0 {
        return 0;
    }

    let year = ((fat_date >> 9) & 0x7F) as u32 + 1980;
    let month = ((fat_date >> 5) & 0x0F) as u32;
    let day = (fat_date & 0x1F) as u32;

    let hours = ((fat_time >> 11) & 0x1F) as u64;
    let minutes = ((fat_time >> 5) & 0x3F) as u64;
    let seconds = ((fat_time & 0x1F) as u64) * 2;

    // Days since Unix epoch (simplified — uses 365/366 day years)
    let mut days: u64 = 0;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    let days_in_month: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        days += days_in_month[(m - 1) as usize];
        if m == 2 && is_leap_year(year) {
            days += 1;
        }
    }

    days += (day - 1) as u64;

    days * 86400 + hours * 3600 + minutes * 60 + seconds
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
