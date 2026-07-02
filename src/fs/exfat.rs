//! exFAT filesystem implementation.
//!
//! This module implements an exFAT (Extended File Allocation Table) driver
//! that reads and writes the on-disk format used by modern Windows systems
//! for removable media. It supports:
//!
//! - Boot sector parsing and validation (exFAT signature "EXFAT   ").
//! - FAT chain traversal for cluster-based file data.
//! - Directory entry parsing (file, stream-extension, name-entry).
//! - File create/read/write, mkdir/rmdir, unlink, rename, symlink/readlink.
//!
//! Block I/O is performed through the [`ExfatBlockDevice`] trait, which
//! abstracts the underlying storage. All multi-byte on-disk fields are
//! little-endian, matching the exFAT specification.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{format, string::{String, ToString}, sync::Arc, vec, vec::Vec};
use spin::RwLock;

// ============================================================================
// Constants
// ============================================================================

const EXFAT_SIGNATURE: &[u8; 8] = b"EXFAT   ";
#[allow(dead_code)]
const EXFAT_ROOT_DIR_CLUSTER: u32 = 0xFFFFFFFF;
const MAX_SYMLINK_DEPTH: usize = 8;
const EXFAT_NAME_MAX: usize = 255;

const ENTRY_UNUSED: u8 = 0x00;
const ENTRY_END_OF_DIR: u8 = 0x80;
const ENTRY_ALLOCATION_BITMAP: u8 = 0x81;
const ENTRY_UPCASE_TABLE: u8 = 0x82;
const ENTRY_VOLUME_LABEL: u8 = 0x83;
const ENTRY_FILE: u8 = 0x85;
const ENTRY_STREAM_EXTENSION: u8 = 0xC0;
const ENTRY_FILE_NAME: u8 = 0xC1;

const FILE_DIR_FLAG: u32 = 0x00000010;
#[allow(dead_code)]
const FILE_SYMLINK_FLAG: u32 = 0x00001000;

const FAT_FREE: u32 = 0x00000000;
const FAT_END_OF_CHAIN: u32 = 0xFFFFFFFF;
const FAT_BAD_CLUSTER: u32 = 0xFFFFFFF7;

// ============================================================================
// Block device trait
// ============================================================================

pub trait ExfatBlockDevice: Send + Sync {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> FsResult<()>;
    fn write_at(&self, offset: u64, buf: &[u8]) -> FsResult<()>;
    #[allow(dead_code)]
    fn size(&self) -> u64;
    fn flush(&self) -> FsResult<()>;
}

// ============================================================================
// On-disk structures
// ============================================================================

#[derive(Debug, Clone)]
struct ExfatBootSector {
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    bytes_per_cluster: u32,
    fat_offset: u32,
    fat_length: u32,
    cluster_heap_offset: u32,
    cluster_count: u32,
    root_dir_cluster: u32,
    volume_flags: u16,
    #[allow(dead_code)]
    bytes_per_sector_shift: u8,
    #[allow(dead_code)]
    sectors_per_cluster_shift: u8,
    #[allow(dead_code)]
    num_fats: u8,
    #[allow(dead_code)]
    drive_select: u8,
    #[allow(dead_code)]
    percent_in_use: u8,
}

impl ExfatBootSector {
    fn parse(buf: &[u8]) -> FsResult<Self> {
        if buf.len() < 512 { return Err(FsError::IoError); }
        if &buf[1..9] != EXFAT_SIGNATURE { return Err(FsError::IoError); }
        let bytes_per_sector_shift = buf[68];
        let sectors_per_cluster_shift = buf[69];
        let bytes_per_sector = 1u32 << bytes_per_sector_shift;
        let sectors_per_cluster = 1u32 << sectors_per_cluster_shift;
        let bytes_per_cluster = bytes_per_sector * sectors_per_cluster;
        let fat_offset = u32::from_le_bytes([buf[80], buf[81], buf[82], buf[83]]);
        let fat_length = u32::from_le_bytes([buf[84], buf[85], buf[86], buf[87]]);
        let cluster_heap_offset = u32::from_le_bytes([buf[88], buf[89], buf[90], buf[91]]);
        let cluster_count = u32::from_le_bytes([buf[92], buf[93], buf[94], buf[95]]);
        let root_dir_cluster = u32::from_le_bytes([buf[96], buf[97], buf[98], buf[99]]);
        let volume_flags = u16::from_le_bytes([buf[106], buf[107]]);
        let num_fats = buf[108];
        let drive_select = buf[109];
        let percent_in_use = buf[110];
        Ok(Self {
            bytes_per_sector, sectors_per_cluster, bytes_per_cluster,
            fat_offset, fat_length, cluster_heap_offset, cluster_count,
            root_dir_cluster, volume_flags,
            bytes_per_sector_shift, sectors_per_cluster_shift,
            num_fats, drive_select, percent_in_use,
        })
    }
}

#[derive(Debug, Clone)]
struct FileEntry {
    secondary_count: u8,
    set_checksum: u16,
    file_attributes: u32,
    timestamp1: u32,
    timestamp2: u32,
    create_time: u8,
    last_access_time: u8,
    modify_time: u8,
    modify_time_tenths: u8,
    create_time_ms: u8,
}

impl FileEntry {
    fn parse(buf: &[u8]) -> FsResult<Self> {
        if buf.len() < 32 { return Err(FsError::IoError); }
        let file_attributes = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        Ok(Self {
            secondary_count: buf[1],
            set_checksum: u16::from_le_bytes([buf[2], buf[3]]),
            file_attributes,
            timestamp1: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
            timestamp2: u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
            create_time: buf[16],
            last_access_time: buf[17],
            modify_time: buf[18],
            modify_time_tenths: buf[19],
            create_time_ms: buf[20],
        })
    }

    fn is_directory(&self) -> bool {
        (self.file_attributes & FILE_DIR_FLAG) != 0
    }

    fn write(&self, buf: &mut [u8]) {
        buf[0] = ENTRY_FILE;
        buf[1] = self.secondary_count;
        buf[2..4].copy_from_slice(&self.set_checksum.to_le_bytes());
        buf[4..8].copy_from_slice(&self.file_attributes.to_le_bytes());
        buf[8..12].copy_from_slice(&self.timestamp1.to_le_bytes());
        buf[12..16].copy_from_slice(&self.timestamp2.to_le_bytes());
        buf[16] = self.create_time;
        buf[17] = self.last_access_time;
        buf[18] = self.modify_time;
        buf[19] = self.modify_time_tenths;
        buf[20] = self.create_time_ms;
    }
}

#[derive(Debug, Clone)]
struct StreamExtensionEntry {
    flags: u8,
    name_length: u8,
    name_hash: u16,
    valid_data_length: u64,
    #[allow(dead_code)]
    reserved: u32,
    first_cluster: u32,
    data_length: u64,
}

impl StreamExtensionEntry {
    fn parse(buf: &[u8]) -> FsResult<Self> {
        if buf.len() < 32 { return Err(FsError::IoError); }
        Ok(Self {
            flags: buf[1],
            name_length: buf[3],
            name_hash: u16::from_le_bytes([buf[4], buf[5]]),
            valid_data_length: u64::from_le_bytes([
                buf[8], buf[9], buf[10], buf[11],
                buf[12], buf[13], buf[14], buf[15],
            ]),
            reserved: u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
            first_cluster: u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]),
            data_length: u64::from_le_bytes([
                buf[24], buf[25], buf[26], buf[27],
                buf[28], buf[29], buf[30], buf[31],
            ]),
        })
    }

    fn has_no_fat_chain(&self) -> bool {
        (self.flags & 0x02) != 0
    }

    fn write(&self, buf: &mut [u8]) {
        buf[0] = ENTRY_STREAM_EXTENSION;
        buf[1] = self.flags;
        buf[3] = self.name_length;
        buf[4..6].copy_from_slice(&self.name_hash.to_le_bytes());
        buf[8..16].copy_from_slice(&self.valid_data_length.to_le_bytes());
        buf[20..24].copy_from_slice(&self.first_cluster.to_le_bytes());
        buf[24..32].copy_from_slice(&self.data_length.to_le_bytes());
    }
}

#[derive(Debug, Clone)]
struct FileNameEntry {
    flags: u8,
    name_part: [u16; 15],
}

impl FileNameEntry {
    fn parse(buf: &[u8]) -> FsResult<Self> {
        if buf.len() < 32 { return Err(FsError::IoError); }
        let mut name_part = [0u16; 15];
        for i in 0..15 {
            let off = 4 + i * 2;
            name_part[i] = u16::from_le_bytes([buf[off], buf[off + 1]]);
        }
        Ok(Self { flags: buf[1], name_part })
    }

    fn write(&self, buf: &mut [u8]) {
        buf[0] = ENTRY_FILE_NAME;
        buf[1] = self.flags;
        for i in 0..15 {
            let off = 4 + i * 2;
            buf[off..off + 2].copy_from_slice(&self.name_part[i].to_le_bytes());
        }
    }
}

// ============================================================================
// Directory entry set (file + stream + names)
// ============================================================================

#[derive(Debug, Clone)]
struct DirEntrySet {
    file_entry: FileEntry,
    stream_entry: StreamExtensionEntry,
    name: String,
    raw_offset: u64,
}

impl DirEntrySet {
    fn inode(&self) -> InodeNumber {
        self.raw_offset
    }

    fn is_directory(&self) -> bool {
        self.file_entry.is_directory()
    }

    fn file_type(&self) -> FileType {
        if self.is_directory() { FileType::Directory }
        else { FileType::Regular }
    }

    fn size(&self) -> u64 {
        self.stream_entry.data_length
    }

    fn first_cluster(&self) -> u32 {
        self.stream_entry.first_cluster
    }
}

// ============================================================================
// Filesystem
// ============================================================================

pub struct ExfatFileSystem {
    device_id: u32,
    device: Arc<dyn ExfatBlockDevice>,
    boot: RwLock<ExfatBootSector>,
    bytes_per_sector: u32,
    bytes_per_cluster: u32,
    fat_offset_bytes: u64,
    cluster_heap_offset_bytes: u64,
    total_clusters: u32,
    root_dir_cluster: u32,
    dirty: RwLock<bool>,
    free_clusters: RwLock<u32>,
}

impl core::fmt::Debug for ExfatFileSystem {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_struct("ExfatFileSystem")
            .field("device_id", &self.device_id)
            .field("bytes_per_cluster", &self.bytes_per_cluster)
            .field("total_clusters", &self.total_clusters)
            .field("root_dir_cluster", &self.root_dir_cluster)
            .finish()
    }
}

impl ExfatFileSystem {
    pub fn new(device_id: u32, device: Arc<dyn ExfatBlockDevice>) -> FsResult<Self> {
        let mut boot_buf = vec![0u8; 512];
        device.read_at(0, &mut boot_buf)?;
        let boot = ExfatBootSector::parse(&boot_buf)?;
        let bytes_per_sector = boot.bytes_per_sector;
        let bytes_per_cluster = boot.bytes_per_cluster;
        let fat_offset_bytes = boot.fat_offset as u64 * bytes_per_sector as u64;
        let cluster_heap_offset_bytes = boot.cluster_heap_offset as u64 * bytes_per_sector as u64;
        let total_clusters = boot.cluster_count;
        let root_dir_cluster = boot.root_dir_cluster;
        let free_clusters = total_clusters / 2;
        Ok(Self {
            device_id, device, boot: RwLock::new(boot),
            bytes_per_sector, bytes_per_cluster,
            fat_offset_bytes, cluster_heap_offset_bytes,
            total_clusters, root_dir_cluster,
            dirty: RwLock::new(false), free_clusters: RwLock::new(free_clusters),
        })
    }

    fn cluster_to_offset(&self, cluster: u32) -> u64 {
        self.cluster_heap_offset_bytes + (cluster as u64 - 2) * self.bytes_per_cluster as u64
    }

    fn read_cluster(&self, cluster: u32, buf: &mut [u8]) -> FsResult<()> {
        if buf.len() < self.bytes_per_cluster as usize {
            return Err(FsError::InvalidArgument);
        }
        let offset = self.cluster_to_offset(cluster);
        self.device.read_at(offset, buf)
    }

    fn write_cluster(&self, cluster: u32, buf: &[u8]) -> FsResult<()> {
        let offset = self.cluster_to_offset(cluster);
        self.device.write_at(offset, buf)
    }

    fn read_fat_entry(&self, cluster: u32) -> FsResult<u32> {
        let offset = self.fat_offset_bytes + cluster as u64 * 4;
        let mut buf = [0u8; 4];
        self.device.read_at(offset, &mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn write_fat_entry(&self, cluster: u32, value: u32) -> FsResult<()> {
        let offset = self.fat_offset_bytes + cluster as u64 * 4;
        let buf = value.to_le_bytes();
        self.device.write_at(offset, &buf)
    }

    fn alloc_cluster(&self) -> FsResult<u32> {
        for cluster in 2..self.total_clusters {
            let entry = self.read_fat_entry(cluster)?;
            if entry == FAT_FREE {
                self.write_fat_entry(cluster, FAT_END_OF_CHAIN)?;
                { let mut fc = self.free_clusters.write(); *fc = fc.saturating_sub(1); }
                return Ok(cluster);
            }
        }
        Err(FsError::NoSpaceLeft)
    }

    fn free_cluster_chain(&self, start_cluster: u32) -> FsResult<()> {
        let mut current = start_cluster;
        let mut count = 0u32;
        loop {
            if current < 2 || current == FAT_END_OF_CHAIN { break; }
            if current == FAT_BAD_CLUSTER { break; }
            let next = self.read_fat_entry(current)?;
            self.write_fat_entry(current, FAT_FREE)?;
            count += 1;
            current = next;
        }
        { let mut fc = self.free_clusters.write(); *fc += count; }
        Ok(())
    }

    fn get_next_cluster(&self, current: u32, no_fat_chain: bool, index: u32) -> FsResult<u32> {
        if no_fat_chain {
            Ok(current + index)
        } else {
            let next = self.read_fat_entry(current)?;
            if next == FAT_END_OF_CHAIN || next < 2 { Err(FsError::IoError) }
            else { Ok(next) }
        }
    }

    fn read_cluster_data(&self, cluster: u32, no_fat_chain: bool, cluster_index: u32, offset_in_cluster: usize, buf: &mut [u8]) -> FsResult<usize> {
        let cluster_size = self.bytes_per_cluster as usize;
        let actual_cluster = if no_fat_chain {
            cluster + cluster_index
        } else {
            let mut cur = cluster;
            for _ in 0..cluster_index {
                cur = self.read_fat_entry(cur)?;
                if cur == FAT_END_OF_CHAIN || cur < 2 { return Err(FsError::IoError); }
            }
            cur
        };
        let mut cluster_buf = vec![0u8; cluster_size];
        self.read_cluster(actual_cluster, &mut cluster_buf)?;
        let available = cluster_size - offset_in_cluster;
        let to_copy = core::cmp::min(buf.len(), available);
        buf[..to_copy].copy_from_slice(&cluster_buf[offset_in_cluster..offset_in_cluster + to_copy]);
        Ok(to_copy)
    }

    fn read_dir_cluster(&self, cluster: u32) -> FsResult<Vec<u8>> {
        let cluster_size = self.bytes_per_cluster as usize;
        let mut result = Vec::new();
        let mut current = cluster;
        loop {
            let mut buf = vec![0u8; cluster_size];
            self.read_cluster(current, &mut buf)?;
            result.extend_from_slice(&buf);
            let next = self.read_fat_entry(current)?;
            if next == FAT_END_OF_CHAIN || next < 2 { break; }
            current = next;
        }
        Ok(result)
    }

    fn parse_dir_entries(&self, data: &[u8]) -> FsResult<Vec<DirEntrySet>> {
        let mut entries = Vec::new();
        let mut pos = 0usize;
        while pos + 32 <= data.len() {
            let entry_type = data[pos];
            if entry_type == ENTRY_END_OF_DIR { break; }
            if entry_type == ENTRY_UNUSED { pos += 32; continue; }
            if entry_type == ENTRY_FILE {
                let file_entry = FileEntry::parse(&data[pos..])?;
                let secondary_count = file_entry.secondary_count as usize;
                if pos + 32 * (1 + secondary_count) > data.len() { break; }
                let mut stream_entry = None;
                let mut name_parts: Vec<Vec<u16>> = Vec::new();
                for i in 1..=secondary_count {
                    let sec_pos = pos + i * 32;
                    let sec_type = data[sec_pos];
                    if sec_type == ENTRY_STREAM_EXTENSION {
                        stream_entry = Some(StreamExtensionEntry::parse(&data[sec_pos..])?);
                    } else if sec_type == ENTRY_FILE_NAME {
                        let fn_entry = FileNameEntry::parse(&data[sec_pos..])?;
                        name_parts.push(fn_entry.name_part.to_vec());
                    }
                }
                if let Some(stream) = stream_entry {
                    let name = reconstruct_name(&name_parts, stream.name_length as usize);
                    entries.push(DirEntrySet {
                        file_entry, stream_entry: stream, name,
                        raw_offset: pos as u64,
                    });
                }
                pos += 32 * (1 + secondary_count);
            } else {
                pos += 32;
            }
        }
        Ok(entries)
    }

    fn dir_lookup(&self, dir_cluster: u32, name: &str) -> FsResult<DirEntrySet> {
        let data = self.read_dir_cluster(dir_cluster)?;
        let entries = self.parse_dir_entries(&data)?;
        for entry in entries {
            if entry.name.eq_ignore_ascii_case(name) {
                return Ok(entry);
            }
        }
        Err(FsError::NotFound)
    }

    fn dir_entries_list(&self, dir_cluster: u32) -> FsResult<Vec<DirEntrySet>> {
        let data = self.read_dir_cluster(dir_cluster)?;
        self.parse_dir_entries(&data)
    }

    fn walk(&self, path: &str, follow_symlink: bool, depth: usize) -> FsResult<DirEntrySet> {
        if depth > MAX_SYMLINK_DEPTH { return Err(FsError::TooManySymlinks); }
        let path = path.trim_start_matches('/');
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        if components.is_empty() {
            return Ok(DirEntrySet {
                file_entry: FileEntry {
                    secondary_count: 0, set_checksum: 0, file_attributes: FILE_DIR_FLAG,
                    timestamp1: 0, timestamp2: 0, create_time: 0, last_access_time: 0,
                    modify_time: 0, modify_time_tenths: 0, create_time_ms: 0,
                },
                stream_entry: StreamExtensionEntry {
                    flags: 0, name_length: 0, name_hash: 0, valid_data_length: 0,
                    reserved: 0, first_cluster: self.root_dir_cluster, data_length: 0,
                },
                name: String::new(), raw_offset: 0,
            });
        }
        let mut current_cluster = self.root_dir_cluster;
        let last = components.len() - 1;
        for (i, comp) in components.iter().enumerate() {
            let entry = self.dir_lookup(current_cluster, comp)?;
            if i < last {
                if entry.is_directory() {
                    current_cluster = entry.first_cluster();
                } else {
                    return Err(FsError::NotADirectory);
                }
            } else if follow_symlink {
                if entry.file_type() == FileType::SymbolicLink {
                    let target = self.read_symlink_target(&entry)?;
                    let resolved = self.walk(&target, true, depth + 1)?;
                    current_cluster = resolved.first_cluster();
                } else if entry.is_directory() {
                    current_cluster = entry.first_cluster();
                }
            }
        }
        Ok(self.dir_lookup(current_cluster, components.last().unwrap())?)
    }

    fn walk_parent(&self, path: &str) -> FsResult<(u32, String)> {
        let path = path.trim_start_matches('/');
        if let Some(idx) = path.rfind('/') {
            let parent = &path[..idx];
            let name = &path[idx + 1..];
            let parent = parent.trim_start_matches('/');
            let parent_cluster = if parent.is_empty() {
                self.root_dir_cluster
            } else {
                let entry = self.walk(parent, true, 0)?;
                if !entry.is_directory() { return Err(FsError::NotADirectory); }
                entry.first_cluster()
            };
            Ok((parent_cluster, name.to_string()))
        } else {
            Ok((self.root_dir_cluster, path.to_string()))
        }
    }

    fn read_file_data(&self, entry: &DirEntrySet, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let data_length = entry.stream_entry.data_length;
        if offset >= data_length { return Ok(0); }
        let remaining = data_length - offset;
        let to_read = core::cmp::min(buffer.len() as u64, remaining) as usize;
        let cluster_size = self.bytes_per_cluster as u64;
        let no_fat_chain = entry.stream_entry.has_no_fat_chain();
        let first_cluster = entry.first_cluster();
        let start_cluster_index = (offset / cluster_size) as u32;
        let offset_in_first = (offset % cluster_size) as usize;
        let mut read = 0usize;
        let mut cluster_index = start_cluster_index;
        let mut offset_in_cluster = offset_in_first;
        while read < to_read {
            let buf_remaining = to_read - read;
            let chunk_size = core::cmp::min(buf_remaining, cluster_size as usize - offset_in_cluster);
            let n = self.read_cluster_data(
                first_cluster, no_fat_chain, cluster_index, offset_in_cluster,
                &mut buffer[read..read + chunk_size],
            )?;
            read += n;
            cluster_index += 1;
            offset_in_cluster = 0;
            if n == 0 { break; }
        }
        Ok(read)
    }

    fn write_file_data(&self, entry: &DirEntrySet, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let cluster_size = self.bytes_per_cluster as u64;
        let no_fat_chain = entry.stream_entry.has_no_fat_chain();
        let first_cluster = entry.first_cluster();
        let data_length = entry.stream_entry.data_length;
        let new_end = offset + buffer.len() as u64;
        let clusters_needed = ((new_end + cluster_size - 1) / cluster_size) as u32;
        let current_clusters = if data_length > 0 {
            ((data_length + cluster_size - 1) / cluster_size) as u32
        } else { 0 };
        let mut actual_first = first_cluster;
        if first_cluster < 2 || first_cluster == FAT_END_OF_CHAIN {
            actual_first = self.alloc_cluster()?;
            if clusters_needed > 1 {
                let mut prev = actual_first;
                for _ in 1..clusters_needed {
                    let next = self.alloc_cluster()?;
                    self.write_fat_entry(prev, next)?;
                    prev = next;
                }
            }
        } else if !no_fat_chain {
            let mut cur = first_cluster;
            let mut count = 1u32;
            loop {
                let next = self.read_fat_entry(cur)?;
                if next == FAT_END_OF_CHAIN || next < 2 { break; }
                cur = next;
                count += 1;
            }
            while count < clusters_needed {
                let new_cluster = self.alloc_cluster()?;
                self.write_fat_entry(cur, new_cluster)?;
                cur = new_cluster;
                count += 1;
            }
        } else if current_clusters < clusters_needed {
            let last_existing = first_cluster + current_clusters - 1;
            let mut prev = last_existing;
            for _ in current_clusters..clusters_needed {
                let new_cluster = self.alloc_cluster()?;
                self.write_fat_entry(prev, new_cluster)?;
                prev = new_cluster;
            }
        }
        let mut written = 0usize;
        let start_cluster_index = (offset / cluster_size) as u32;
        let offset_in_first = (offset % cluster_size) as usize;
        let mut cluster_index = start_cluster_index;
        let mut offset_in_cluster = offset_in_first;
        while written < buffer.len() {
            let buf_remaining = buffer.len() - written;
            let chunk_size = core::cmp::min(buf_remaining, cluster_size as usize - offset_in_cluster);
            let actual_cluster = if no_fat_chain || (first_cluster == actual_first && no_fat_chain) {
                actual_first + cluster_index
            } else {
                let mut cur = actual_first;
                for _ in 0..cluster_index {
                    cur = self.read_fat_entry(cur)?;
                    if cur == FAT_END_OF_CHAIN || cur < 2 { return Err(FsError::IoError); }
                }
                cur
            };
            let mut cluster_buf = vec![0u8; cluster_size as usize];
            self.read_cluster(actual_cluster, &mut cluster_buf)?;
            cluster_buf[offset_in_cluster..offset_in_cluster + chunk_size]
                .copy_from_slice(&buffer[written..written + chunk_size]);
            self.write_cluster(actual_cluster, &cluster_buf)?;
            written += chunk_size;
            cluster_index += 1;
            offset_in_cluster = 0;
        }
        if new_end > data_length {
            self.update_dir_entry_data_length(entry, new_end, actual_first)?;
        }
        Ok(written)
    }

    fn update_dir_entry_data_length(&self, entry: &DirEntrySet, new_length: u64, first_cluster: u32) -> FsResult<()> {
        let dir_cluster = if entry.raw_offset == 0 {
            self.root_dir_cluster
        } else {
            self.root_dir_cluster
        };
        let data = self.read_dir_cluster(dir_cluster)?;
        let mut new_data = data.clone();
        let offset = entry.raw_offset as usize;
        if offset + 64 > new_data.len() { return Err(FsError::IoError); }
        let stream_off = offset + 32;
        new_data[stream_off + 24..stream_off + 32].copy_from_slice(&new_length.to_le_bytes());
        new_data[stream_off + 8..stream_off + 16].copy_from_slice(&new_length.to_le_bytes());
        new_data[stream_off + 20..stream_off + 24].copy_from_slice(&first_cluster.to_le_bytes());
        self.write_dir_cluster(dir_cluster, &new_data)?;
        Ok(())
    }

    fn write_dir_cluster(&self, dir_cluster: u32, data: &[u8]) -> FsResult<()> {
        let cluster_size = self.bytes_per_cluster as usize;
        let mut current = dir_cluster;
        let mut offset = 0usize;
        loop {
            let end = core::cmp::min(offset + cluster_size, data.len());
            let chunk = &data[offset..end];
            let mut buf = vec![0u8; cluster_size];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.write_cluster(current, &buf)?;
            offset = end;
            if offset >= data.len() { break; }
            let next = self.read_fat_entry(current)?;
            if next == FAT_END_OF_CHAIN || next < 2 {
                let new_cluster = self.alloc_cluster()?;
                self.write_fat_entry(current, new_cluster)?;
                current = new_cluster;
            } else {
                current = next;
            }
        }
        Ok(())
    }

    fn append_dir_entry(&self, dir_cluster: u32, entry_data: &[u8]) -> FsResult<u64> {
        let data = self.read_dir_cluster(dir_cluster)?;
        let cluster_size = self.bytes_per_cluster as usize;
        let mut insert_pos = None;
        for i in (0..data.len()).step_by(32) {
            if i + 32 > data.len() { break; }
            if data[i] == ENTRY_END_OF_DIR || data[i] == ENTRY_UNUSED {
                insert_pos = Some(i);
                break;
            }
        }
        let pos = insert_pos.unwrap_or(data.len());
        let mut new_data = data.clone();
        let needed_end = pos + entry_data.len() + 32;
        if needed_end > new_data.len() {
            new_data.resize(needed_end, 0);
        }
        new_data[pos..pos + entry_data.len()].copy_from_slice(entry_data);
        let end_marker_pos = pos + entry_data.len();
        if end_marker_pos + 32 <= new_data.len() {
            new_data[end_marker_pos] = ENTRY_END_OF_DIR;
        }
        self.write_dir_cluster(dir_cluster, &new_data)?;
        Ok(pos as u64)
    }

    fn create_entry_set(
        &self, dir_cluster: u32, name: &str, is_dir: bool, first_cluster: u32, data_length: u64,
    ) -> FsResult<DirEntrySet> {
        let name_utf16: Vec<u16> = name.encode_utf16().collect();
        let name_len = name_utf16.len();
        if name_len > EXFAT_NAME_MAX { return Err(FsError::NameTooLong); }
        let name_hash = compute_name_hash(&name_utf16);
        let num_name_entries = (name_len + 14) / 15;
        let secondary_count = 1 + num_name_entries as u8;
        let now = get_current_time();
        let fat_timestamp = unix_to_fat_timestamp(now);
        let file_entry = FileEntry {
            secondary_count,
            set_checksum: 0,
            file_attributes: if is_dir { FILE_DIR_FLAG } else { 0 },
            timestamp1: fat_timestamp,
            timestamp2: fat_timestamp,
            create_time: 0,
            last_access_time: 0,
            modify_time: 0,
            modify_time_tenths: 0,
            create_time_ms: 0,
        };
        let stream_entry = StreamExtensionEntry {
            flags: if first_cluster < 2 { 0x01 } else { 0x03 },
            name_length: name_len as u8,
            name_hash,
            valid_data_length: data_length,
            reserved: 0,
            first_cluster,
            data_length,
        };
        let mut entry_buf = Vec::new();
        let mut file_buf = [0u8; 32];
        file_entry.write(&mut file_buf);
        entry_buf.extend_from_slice(&file_buf);
        let mut stream_buf = [0u8; 32];
        stream_entry.write(&mut stream_buf);
        entry_buf.extend_from_slice(&stream_buf);
        for i in 0..num_name_entries {
            let mut name_buf = [0u8; 32];
            let mut name_part = [0u16; 15];
            let start = i * 15;
            let end = core::cmp::min(start + 15, name_len);
            for j in 0..(end - start) { name_part[j] = name_utf16[start + j]; }
            let fn_entry = FileNameEntry { flags: i as u8, name_part };
            fn_entry.write(&mut name_buf);
            entry_buf.extend_from_slice(&name_buf);
        }
        let raw_offset = self.append_dir_entry(dir_cluster, &entry_buf)?;
        Ok(DirEntrySet {
            file_entry, stream_entry, name: name.to_string(), raw_offset,
        })
    }

    fn remove_dir_entry(&self, dir_cluster: u32, name: &str) -> FsResult<DirEntrySet> {
        let data = self.read_dir_cluster(dir_cluster)?;
        let entries = self.parse_dir_entries(&data)?;
        let entry = entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))
            .ok_or(FsError::NotFound)?;
        let entry_clone = entry.clone();
        let mut new_data = data.clone();
        let offset = entry.raw_offset as usize;
        let total_len = 32 * (1 + entry.file_entry.secondary_count as usize);
        if offset + total_len <= new_data.len() {
            for i in 0..total_len { new_data[offset + i] = ENTRY_UNUSED; }
        }
        self.write_dir_cluster(dir_cluster, &new_data)?;
        Ok(entry_clone)
    }

    fn metadata_from_entry(&self, entry: &DirEntrySet) -> FileMetadata {
        let file_type = entry.file_type();
        let fat_to_unix_ms = |ts: u32| -> u64 {
            if ts == 0 { return 0; }
            let fat_epoch = (ts as u64) / 2;
            fat_epoch_to_unix_ms(fat_epoch)
        };
        FileMetadata {
            inode: entry.inode(),
            file_type,
            size: entry.size(),
            permissions: if file_type == FileType::Directory {
                FilePermissions::default_directory()
            } else {
                FilePermissions::default_file()
            },
            uid: 0, gid: 0,
            created: fat_to_unix_ms(entry.file_entry.timestamp1),
            modified: fat_to_unix_ms(entry.file_entry.timestamp2),
            accessed: fat_to_unix_ms(entry.file_entry.timestamp2),
            link_count: 1,
            device_id: Some(self.device_id),
        }
    }

    fn read_symlink_target(&self, entry: &DirEntrySet) -> FsResult<String> {
        let mut buf = vec![0u8; entry.size() as usize];
        let n = self.read_file_data(entry, 0, &mut buf)?;
        let mut s = String::new();
        let mut i = 0;
        while i + 1 < n {
            let cu = u16::from_le_bytes([buf[i], buf[i + 1]]);
            if cu == 0 { break; }
            s.push(char::from_u32(cu as u32).unwrap_or('?'));
            i += 2;
        }
        Ok(s)
    }

    fn is_symlink_entry(&self, entry: &DirEntrySet) -> bool {
        if entry.is_directory() { return false; }
        if entry.size() == 0 { return false; }
        if entry.size() > 4096 { return false; }
        let mut buf = [0u8; 16];
        if self.read_file_data(entry, 0, &mut buf).is_err() { return false; }
        buf[0..8] == *b"SYMLINK\0"
    }

    fn split_path(path: &str) -> (String, String) {
        let path = path.trim_start_matches('/');
        if let Some(idx) = path.rfind('/') {
            let parent = &path[..idx];
            let name = &path[idx + 1..];
            let parent = parent.trim_start_matches('/');
            let parent = if parent.is_empty() { "/".to_string() } else { format!("/{}", parent) };
            (parent, name.to_string())
        } else {
            ("/".to_string(), path.to_string())
        }
    }
}

fn reconstruct_name(parts: &[Vec<u16>], total_len: usize) -> String {
    let mut all: Vec<u16> = Vec::new();
    for part in parts { all.extend_from_slice(part); }
    all.truncate(total_len);
    let mut s = String::new();
    for cu in all {
        if cu == 0 { break; }
        s.push(char::from_u32(cu as u32).unwrap_or('?'));
    }
    s
}

fn compute_name_hash(name_utf16: &[u16]) -> u16 {
    let mut hash: u16 = 0;
    for &cu in name_utf16 {
        let upper = if cu <= 0x7F {
            (cu as u8).to_ascii_uppercase() as u16
        } else {
            cu
        };
        hash = hash.wrapping_add(upper);
        hash = hash.wrapping_mul(3);
        hash = hash.wrapping_add(hash >> 11);
    }
    hash
}

fn unix_to_fat_timestamp(unix_ms: u64) -> u32 {
    let unix_s = unix_ms / 1000;
    let fat_epoch = (unix_s as i64 - 315532800).max(0) as u64;
    let fat_time = fat_epoch * 2;
    if fat_time > 0xFFFFFFFF { 0xFFFFFFFF as u32 } else { fat_time as u32 }
}

fn fat_epoch_to_unix_ms(fat_epoch: u64) -> u64 {
    (fat_epoch + 315532800) * 1000
}

impl FileSystem for ExfatFileSystem {
    fn fs_type(&self) -> FileSystemType { FileSystemType::ExFat }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let free = *self.free_clusters.read();
        Ok(FileSystemStats {
            total_blocks: self.total_clusters as u64,
            free_blocks: free as u64,
            available_blocks: free as u64,
            total_inodes: 0,
            free_inodes: 0,
            block_size: self.bytes_per_cluster,
            max_filename_length: EXFAT_NAME_MAX as u32,
        })
    }

    fn create(&self, path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = Self::split_path(path);
        if name.is_empty() || name == "." || name == ".." { return Err(FsError::InvalidArgument); }
        let parent_entry = self.walk(&parent_path, true, 0)?;
        if !parent_entry.is_directory() { return Err(FsError::NotADirectory); }
        let parent_cluster = parent_entry.first_cluster();
        if self.dir_lookup(parent_cluster, &name).is_ok() { return Err(FsError::AlreadyExists); }
        let entry = self.create_entry_set(parent_cluster, &name, false, 0, 0)?;
        Ok(entry.inode())
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        if flags.create {
            match self.walk(path, true, 0) {
                Ok(entry) => {
                    if flags.exclusive { return Err(FsError::AlreadyExists); }
                    if entry.is_directory() { return Err(FsError::IsADirectory); }
                    if flags.truncate {
                        if entry.first_cluster() >= 2 {
                            self.free_cluster_chain(entry.first_cluster())?;
                        }
                        self.update_dir_entry_data_length(&entry, 0, 0)?;
                    }
                    return Ok(entry.inode());
                }
                Err(FsError::NotFound) => { return self.create(path, FilePermissions::default_file()); }
                Err(e) => return Err(e),
            }
        }
        let entry = self.walk(path, true, 0)?;
        if entry.is_directory() { return Err(FsError::IsADirectory); }
        if flags.truncate {
            if entry.first_cluster() >= 2 {
                self.free_cluster_chain(entry.first_cluster())?;
            }
            self.update_dir_entry_data_length(&entry, 0, 0)?;
        }
        Ok(entry.inode())
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let root_data = self.read_dir_cluster(self.root_dir_cluster)?;
        let entries = self.parse_dir_entries(&root_data)?;
        let entry = entries.iter().find(|e| e.inode() == inode)
            .ok_or(FsError::NotFound)?;
        if entry.is_directory() { return Err(FsError::IsADirectory); }
        self.read_file_data(entry, offset, buffer)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let root_data = self.read_dir_cluster(self.root_dir_cluster)?;
        let entries = self.parse_dir_entries(&root_data)?;
        let entry = entries.iter().find(|e| e.inode() == inode)
            .ok_or(FsError::NotFound)?;
        if entry.is_directory() { return Err(FsError::IsADirectory); }
        self.write_file_data(entry, offset, buffer)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let root_data = self.read_dir_cluster(self.root_dir_cluster)?;
        let entries = self.parse_dir_entries(&root_data)?;
        let entry = entries.iter().find(|e| e.inode() == inode)
            .ok_or(FsError::NotFound)?;
        Ok(self.metadata_from_entry(entry))
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let root_data = self.read_dir_cluster(self.root_dir_cluster)?;
        let entries = self.parse_dir_entries(&root_data)?;
        let entry = entries.iter().find(|e| e.inode() == inode)
            .ok_or(FsError::NotFound)?;
        if metadata.size != entry.size() {
            if metadata.size == 0 {
                if entry.first_cluster() >= 2 {
                    self.free_cluster_chain(entry.first_cluster())?;
                }
                self.update_dir_entry_data_length(entry, 0, 0)?;
            }
        }
        Ok(())
    }

    fn mkdir(&self, path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = Self::split_path(path);
        if name.is_empty() || name == "." || name == ".." { return Err(FsError::InvalidArgument); }
        let parent_entry = self.walk(&parent_path, true, 0)?;
        if !parent_entry.is_directory() { return Err(FsError::NotADirectory); }
        let parent_cluster = parent_entry.first_cluster();
        if self.dir_lookup(parent_cluster, &name).is_ok() { return Err(FsError::AlreadyExists); }
        let new_cluster = self.alloc_cluster()?;
        let cluster_size = self.bytes_per_cluster as usize;
        let mut buf = vec![0u8; cluster_size];
        buf[0] = ENTRY_END_OF_DIR;
        self.write_cluster(new_cluster, &buf)?;
        let entry = self.create_entry_set(parent_cluster, &name, true, new_cluster, cluster_size as u64)?;
        Ok(entry.inode())
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let entry = self.walk(path, true, 0)?;
        if !entry.is_directory() { return Err(FsError::NotADirectory); }
        let dir_cluster = entry.first_cluster();
        let sub_entries = self.dir_entries_list(dir_cluster)?;
        if !sub_entries.is_empty() { return Err(FsError::DirectoryNotEmpty); }
        let (parent_path, name) = Self::split_path(path);
        let parent_entry = self.walk(&parent_path, true, 0)?;
        let parent_cluster = parent_entry.first_cluster();
        self.remove_dir_entry(parent_cluster, &name)?;
        if dir_cluster >= 2 { self.free_cluster_chain(dir_cluster)?; }
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let entry = self.walk(path, true, 0)?;
        if entry.is_directory() { return Err(FsError::IsADirectory); }
        let (parent_path, name) = Self::split_path(path);
        let parent_entry = self.walk(&parent_path, true, 0)?;
        let parent_cluster = parent_entry.first_cluster();
        self.remove_dir_entry(parent_cluster, &name)?;
        if entry.first_cluster() >= 2 { self.free_cluster_chain(entry.first_cluster())?; }
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let root_data = self.read_dir_cluster(self.root_dir_cluster)?;
        let entries = self.parse_dir_entries(&root_data)?;
        let entry = entries.iter().find(|e| e.inode() == inode)
            .ok_or(FsError::NotFound)?;
        if !entry.is_directory() { return Err(FsError::NotADirectory); }
        let dir_cluster = entry.first_cluster();
        let sub_entries = self.dir_entries_list(dir_cluster)?;
        let mut result = Vec::new();
        for e in sub_entries {
            let inode_num = e.inode();
            let name = e.name.clone();
            let ft = e.file_type();
            result.push(DirectoryEntry { name, inode: inode_num, file_type: ft });
        }
        Ok(result)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let (old_parent_path, old_name) = Self::split_path(old_path);
        let (new_parent_path, new_name) = Self::split_path(new_path);
        let old_parent_entry = self.walk(&old_parent_path, true, 0)?;
        let new_parent_entry = self.walk(&new_parent_path, true, 0)?;
        let old_parent_cluster = old_parent_entry.first_cluster();
        let new_parent_cluster = new_parent_entry.first_cluster();
        let src_entry = self.dir_lookup(old_parent_cluster, &old_name)?;
        if self.dir_lookup(new_parent_cluster, &new_name).is_ok() {
            self.remove_dir_entry(new_parent_cluster, &new_name)?;
        }
        let new_entry = self.create_entry_set(
            new_parent_cluster, &new_name,
            src_entry.is_directory(),
            src_entry.first_cluster(),
            src_entry.size(),
        )?;
        self.remove_dir_entry(old_parent_cluster, &old_name)?;
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_path, name) = Self::split_path(link_path);
        if name.is_empty() { return Err(FsError::InvalidArgument); }
        let parent_entry = self.walk(&parent_path, true, 0)?;
        let parent_cluster = parent_entry.first_cluster();
        if self.dir_lookup(parent_cluster, &name).is_ok() { return Err(FsError::AlreadyExists); }
        let target_utf16: Vec<u16> = target.encode_utf16().collect();
        let target_bytes: Vec<u8> = target_utf16.iter().flat_map(|cu| cu.to_le_bytes()).collect();
        let mut file_data = b"SYMLINK\0".to_vec();
        file_data.extend_from_slice(&target_bytes);
        file_data.extend_from_slice(&[0, 0]);
        let entry = self.create_entry_set(parent_cluster, &name, false, 0, 0)?;
        self.write_file_data(&entry, 0, &file_data)?;
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let entry = self.walk(path, false, 0)?;
        if !self.is_symlink_entry(&entry) { return Err(FsError::InvalidArgument); }
        self.read_symlink_target(&entry)
    }

    fn sync(&self) -> FsResult<()> {
        if *self.dirty.read() { *self.dirty.write() = false; }
        self.device.flush()
    }
}
