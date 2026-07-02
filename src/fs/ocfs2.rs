//! OCFS2 (Oracle Cluster File System 2) implementation — single-node mode.
//!
//! OCFS2 is a general-purpose cluster filesystem designed for RAC (Real
//! Application Cluster) environments. This driver implements the on-disk
//! format for single-node (local) mode, without the distributed lock manager
//! or cluster heartbeat subsystem. The on-disk structures are fully parsed
//! and used for real read/write operations.
//!
//! On-disk layout (all multi-byte fields are little-endian):
//!
//! ```text
//! Superblock (block 0, offset 0):
//!   +0   | i_signature     | "OCFSV2\0\0" (8 bytes, first u32 = 0x58534f43)
//!   +... | (full superblock is ~4096 bytes; key fields below)
//!
//! Key superblock fields (offsets from start of superblock):
//!   +0x00  | s_signature    | u32 = 0x58534f43 ("OCFS")
//!   +0x04  | s_major_rev    | u32
//!   +0x08  | s_minor_rev    | u32
//!   +0x0C  | s_volume_state | u32
//!   +0x10  | s_block_size   | u32 (bytes)
//!   +0x14  | s_cluster_size | u32 (bytes, multiple of block_size)
//!   +0x18  | s_max_slots    | u32
//!   +0x1C  | s_root_blkno   | u64 (block number of root directory inode)
//!   +0x24  | s_system_dir_blkno | u64 (block number of system directory)
//!   +0x2C  | s_total_clusters| u32
//!   +0x30  | s_free_clusters | u32
//!
//! Inode (on-disk, variable size, starts at block-aligned offset):
//!   +0x00  | i_signature    | u32 = 0x494e4f44 ("DINO")
//!   +0x04  | i_size         | u64
//!   +0x0C  | i_blkno        | u64 (block number of this inode)
//!   +0x14  | i_flags        | u32
//!   +0x18  | i_links_count  | u16
//!   +0x1A  | i_mode         | u16 (file type + permissions)
//!   +0x1C  | i_uid          | u32
//!   +0x20  | i_gid          | u32
//!   +0x24  | i_atime        | u64
//!   +0x2C  | i_ctime        | u64
//!   +0x34  | i_mtime        | u64
//!   +0x3C  | i_dtime        | u64
//!   +0x44  | i_blocks       | u64 (in 512-byte sectors)
//!   +0x4C  | i_data_start   | u64 (block number of inline data / extent list)
//!   +0x54  | i_clusters      | u32
//!   +0x58  | i_attr         | u32
//!   +0x5C  | i_dyn_features | u32
//!   +0x60  | i_data         | [u32; 50] — inline data or extent descriptors
//!
//! Directory entries (within inode inline data or extent blocks):
//!   +0x00  | de_inode       | u64
//!   +0x08  | de_rec_len     | u16
//!   +0x0A  | de_name_len    | u8
//!   +0x0B  | de_file_type   | u8
//!   +0x0C  | de_name        | variable (null-terminated, padded to rec_len)
//! ```
//!
//! See: linux-master/fs/ocfs2/

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use crate::drivers::storage::{read_storage_sectors, write_storage_sectors};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use spin::RwLock;

/// OCFS2 superblock magic: first 4 bytes of "OCFSV2\0\0" = 0x58534f43.
const OCFS2_MAGIC: u32 = 0x58534f43;
/// OCFS2 inode magic: "DINO" = 0x494e4f44.
const OCFS2_INODE_MAGIC: u32 = 0x494e4f44;

/// File type bits in inode mode (same as standard Linux S_IFMT values).
const S_IFMT: u16 = 0xF000;
const S_IFREG: u16 = 0x8000;
const S_IFDIR: u16 = 0x4000;
const S_IFLNK: u16 = 0xA000;
const S_IFCHR: u16 = 0x2000;
const S_IFBLK: u16 = 0x6000;
const S_IFIFO: u16 = 0x1000;
const S_IFSOCK: u16 = 0xC000;

/// Directory entry file type values (de_file_type field).
#[allow(dead_code)]
const OCFS2_FT_UNKNOWN: u8 = 0;
const OCFS2_FT_REG_FILE: u8 = 1;
const OCFS2_FT_DIR: u8 = 2;
const OCFS2_FT_SYMLINK: u8 = 7;

/// Maximum number of symlink hops during path resolution.
const MAX_SYMLINK_DEPTH: usize = 8;

/// Maximum inline data entries in the inode (i_data array).
const OCFS2_INLINE_DATA_ENTRIES: usize = 50;

/// Parsed OCFS2 superblock (key fields only).
#[derive(Debug, Clone)]
struct Ocfs2Super {
    block_size: u32,
    cluster_size: u32,
    #[allow(dead_code)]
    max_slots: u32,
    root_blkno: u64,
    #[allow(dead_code)]
    system_dir_blkno: u64,
    total_clusters: u32,
    free_clusters: u32,
}

/// In-memory representation of an OCFS2 inode.
#[derive(Debug, Clone)]
struct Ocfs2Inode {
    /// Block number where this inode lives on disk.
    blkno: u64,
    size: u64,
    mode: u16,
    uid: u32,
    gid: u32,
    links_count: u16,
    atime: u64,
    ctime: u64,
    mtime: u64,
    /// Inline data or extent block numbers.
    data: [u32; OCFS2_INLINE_DATA_ENTRIES],
    /// Cached directory entries (for directories).
    entries: Vec<Ocfs2DirEntry>,
}

/// A parsed directory entry.
#[derive(Debug, Clone)]
struct Ocfs2DirEntry {
    inode_blkno: u64,
    name: String,
    file_type: u8,
}

/// OCFS2 filesystem instance (single-node mode).
#[derive(Debug)]
pub struct Ocfs2FileSystem {
    device_id: u32,
    sector_base: u64,
    superblock: Ocfs2Super,
    /// Inode cache: blkno → parsed inode.
    inode_cache: RwLock<BTreeMap<u64, Ocfs2Inode>>,
    /// Block cache: block number → block data.
    block_cache: RwLock<BTreeMap<u64, Vec<u8>>>,
    /// Next free block number for allocation (simplified single-node allocator).
    next_free_block: RwLock<u64>,
}

impl Ocfs2FileSystem {
    /// Create a new OCFS2 filesystem instance, reading the superblock from
    /// the given storage device.
    pub fn new(device_id: u32) -> FsResult<Self> {
        Self::new_at(device_id, 0)
    }

    /// Open OCFS2 on a partition starting at `sector_base` (512-byte sectors).
    pub fn new_at(device_id: u32, sector_base: u64) -> FsResult<Self> {
        let mut fs = Self {
            device_id,
            sector_base,
            superblock: Ocfs2Super {
                block_size: 4096,
                cluster_size: 4096,
                max_slots: 1,
                root_blkno: 0,
                system_dir_blkno: 0,
                total_clusters: 0,
                free_clusters: 0,
            },
            inode_cache: RwLock::new(BTreeMap::new()),
            block_cache: RwLock::new(BTreeMap::new()),
            next_free_block: RwLock::new(100), // Start allocation after metadata area
        };

        fs.read_superblock()?;
        Ok(fs)
    }

    /// Read and validate the OCFS2 superblock from disk.
    fn read_superblock(&mut self) -> FsResult<()> {
        // The superblock is in the first block of the filesystem.
        let block_size = 4096u32; // Initial assumption; OCFS2 min block size is 512
        let sectors_per_block = block_size / 512;
        let mut buffer = vec![0u8; block_size as usize];

        read_storage_sectors(self.device_id, self.sector_base, &mut buffer)
            .map_err(|_| FsError::IoError)?;

        // Check magic at offset 0.
        let magic = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
        if magic != OCFS2_MAGIC {
            return Err(FsError::InvalidArgument);
        }

        // Parse key fields (offsets as documented above).
        let s_block_size = read_u32_le(&buffer, 0x10);
        let s_cluster_size = read_u32_le(&buffer, 0x14);
        let s_max_slots = read_u32_le(&buffer, 0x18);
        let s_root_blkno = read_u64_le(&buffer, 0x1C);
        let s_system_dir_blkno = read_u64_le(&buffer, 0x24);
        let s_total_clusters = read_u32_le(&buffer, 0x2C);
        let s_free_clusters = read_u32_le(&buffer, 0x30);

        // Validate block size: must be a power of 2 between 512 and 4096.
        if s_block_size < 512 || s_block_size > 4096 || !s_block_size.is_power_of_two() {
            return Err(FsError::InvalidArgument);
        }
        // Cluster size must be a multiple of block size.
        if s_cluster_size == 0 || s_cluster_size % s_block_size != 0 {
            return Err(FsError::InvalidArgument);
        }

        self.superblock = Ocfs2Super {
            block_size: s_block_size,
            cluster_size: s_cluster_size,
            max_slots: s_max_slots,
            root_blkno: s_root_blkno,
            system_dir_blkno: s_system_dir_blkno,
            total_clusters: s_total_clusters,
            free_clusters: s_free_clusters,
        };

        Ok(())
    }

    /// Read a raw block from disk into a Vec, with caching.
    fn read_block(&self, block_num: u64) -> FsResult<Vec<u8>> {
        {
            let cache = self.block_cache.read();
            if let Some(cached) = cache.get(&block_num) {
                return Ok(cached.clone());
            }
        }

        let block_size = self.superblock.block_size as u64;
        let sectors_per_block = self.superblock.block_size / 512;
        let start_sector = block_num * sectors_per_block as u64;
        let mut buffer = vec![0u8; block_size as usize];

        read_storage_sectors(self.device_id, self.sector_base + start_sector, &mut buffer)
            .map_err(|_| FsError::IoError)?;

        let mut cache = self.block_cache.write();
        cache.insert(block_num, buffer.clone());
        Ok(buffer)
    }

    /// Write a raw block to disk and update the cache.
    fn write_block(&self, block_num: u64, data: &[u8]) -> FsResult<()> {
        let block_size = self.superblock.block_size as usize;
        if data.len() != block_size {
            return Err(FsError::InvalidArgument);
        }

        let sectors_per_block = self.superblock.block_size / 512;
        let start_sector = block_num * sectors_per_block as u64;

        write_storage_sectors(self.device_id, self.sector_base + start_sector, data)
            .map_err(|_| FsError::IoError)?;

        let mut cache = self.block_cache.write();
        cache.insert(block_num, data.to_vec());
        Ok(())
    }

    /// Read and parse an inode from the given block number.
    fn read_inode_from_blkno(&self, blkno: u64) -> FsResult<Ocfs2Inode> {
        {
            let cache = self.inode_cache.read();
            if let Some(cached) = cache.get(&blkno) {
                return Ok(cached.clone());
            }
        }

        let block = self.read_block(blkno)?;

        // Check inode magic.
        let magic = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
        if magic != OCFS2_INODE_MAGIC {
            return Err(FsError::InvalidArgument);
        }

        let size = read_u64_le(&block, 0x04);
        let inode_blkno = read_u64_le(&block, 0x0C);
        let links_count = read_u16_le(&block, 0x18);
        let mode = read_u16_le(&block, 0x1A);
        let uid = read_u32_le(&block, 0x1C);
        let gid = read_u32_le(&block, 0x20);
        let atime = read_u64_le(&block, 0x24);
        let ctime = read_u64_le(&block, 0x2C);
        let mtime = read_u64_le(&block, 0x34);

        let mut data = [0u32; OCFS2_INLINE_DATA_ENTRIES];
        for i in 0..OCFS2_INLINE_DATA_ENTRIES {
            data[i] = read_u32_le(&block, 0x60 + i * 4);
        }

        // Parse directory entries if this is a directory.
        let mut entries = Vec::new();
        if (mode & S_IFMT) == S_IFDIR {
            entries = self.parse_dir_entries(&block, 0x60 + OCFS2_INLINE_DATA_ENTRIES * 4, size)?;
        }

        let inode = Ocfs2Inode {
            blkno: inode_blkno,
            size,
            mode,
            uid,
            gid,
            links_count,
            atime,
            ctime,
            mtime,
            data,
            entries,
        };

        let mut cache = self.inode_cache.write();
        cache.insert(blkno, inode.clone());
        Ok(inode)
    }

    /// Parse directory entries from a buffer starting at `offset`.
    fn parse_dir_entries(
        &self,
        buf: &[u8],
        start: usize,
        _dir_size: u64,
    ) -> FsResult<Vec<Ocfs2DirEntry>> {
        let mut entries = Vec::new();
        let mut pos = start;

        while pos + 12 <= buf.len() {
            let inode_blkno = read_u64_le(buf, pos);
            let rec_len = read_u16_le(buf, pos + 8) as usize;
            let name_len = buf[pos + 10] as usize;
            let file_type = buf[pos + 11];

            if rec_len == 0 || rec_len < 12 || pos + rec_len > buf.len() {
                break;
            }

            if inode_blkno != 0 && name_len > 0 {
                let name_end = pos + 12 + core::cmp::min(name_len, rec_len - 12);
                let name_end = core::cmp::min(name_end, buf.len());
                let name_bytes = &buf[pos + 12..name_end];
                // Trim null bytes.
                let trimmed = name_bytes
                    .iter()
                    .take_while(|&&b| b != 0)
                    .copied()
                    .collect::<Vec<u8>>();
                if let Ok(name_str) = alloc::str::from_utf8(&trimmed) {
                    if !name_str.is_empty() {
                        entries.push(Ocfs2DirEntry {
                            inode_blkno,
                            name: name_str.to_string(),
                            file_type,
                        });
                    }
                }
            }

            pos += rec_len;
        }

        Ok(entries)
    }

    /// Write an inode back to disk.
    fn write_inode(&self, inode: &Ocfs2Inode) -> FsResult<()> {
        let block_size = self.superblock.block_size as usize;
        let mut block = self.read_block(inode.blkno)?;

        // Write inode fields.
        write_u32_le(&mut block, 0x00, OCFS2_INODE_MAGIC);
        write_u64_le(&mut block, 0x04, inode.size);
        write_u64_le(&mut block, 0x0C, inode.blkno);
        write_u16_le(&mut block, 0x18, inode.links_count);
        write_u16_le(&mut block, 0x1A, inode.mode);
        write_u32_le(&mut block, 0x1C, inode.uid);
        write_u32_le(&mut block, 0x20, inode.gid);
        write_u64_le(&mut block, 0x24, inode.atime);
        write_u64_le(&mut block, 0x2C, inode.ctime);
        write_u64_le(&mut block, 0x34, inode.mtime);

        for i in 0..OCFS2_INLINE_DATA_ENTRIES {
            write_u32_le(&mut block, 0x60 + i * 4, inode.data[i]);
        }

        // Write directory entries if this is a directory.
        if (inode.mode & S_IFMT) == S_IFDIR {
            self.write_dir_entries(
                &mut block,
                0x60 + OCFS2_INLINE_DATA_ENTRIES * 4,
                block_size,
                &inode.entries,
            )?;
        }

        self.write_block(inode.blkno, &block)?;

        let mut cache = self.inode_cache.write();
        cache.insert(inode.blkno, inode.clone());
        Ok(())
    }

    /// Serialize directory entries into a buffer.
    fn write_dir_entries(
        &self,
        buf: &mut [u8],
        start: usize,
        buf_len: usize,
        entries: &[Ocfs2DirEntry],
    ) -> FsResult<()> {
        let mut pos = start;
        for entry in entries {
            let name_bytes = entry.name.as_bytes();
            let name_len = name_bytes.len();
            // rec_len = 12 + name_len, padded to 4-byte boundary.
            let rec_len = ((12 + name_len + 3) & !3) as usize;
            if pos + rec_len > buf_len {
                return Err(FsError::NoSpaceLeft);
            }

            write_u64_le(buf, pos, entry.inode_blkno);
            write_u16_le(buf, pos + 8, rec_len as u16);
            buf[pos + 10] = name_len as u8;
            buf[pos + 11] = entry.file_type;
            // Write name and zero-pad.
            let name_end = pos + 12 + name_len;
            buf[pos + 12..name_end].copy_from_slice(name_bytes);
            for i in name_end..pos + rec_len {
                if i < buf_len {
                    buf[i] = 0;
                }
            }
            pos += rec_len;
        }
        Ok(())
    }

    /// Resolve a path to an inode block number.
    fn resolve_path(&self, path: &str) -> FsResult<u64> {
        self.resolve_path_depth(path, 0)
    }

    fn resolve_path_depth(&self, path: &str, depth: usize) -> FsResult<u64> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(FsError::TooManySymlinks);
        }

        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Ok(self.superblock.root_blkno);
        }

        let mut current_blkno = self.superblock.root_blkno;
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        for (i, component) in components.iter().enumerate() {
            let inode = self.read_inode_from_blkno(current_blkno)?;

            if (inode.mode & S_IFMT) != S_IFDIR {
                return Err(FsError::NotADirectory);
            }

            let found = inode
                .entries
                .iter()
                .find(|e| e.name == *component)
                .ok_or(FsError::NotFound)?;

            // If this is a symlink and not the last component, follow it.
            if found.file_type == OCFS2_FT_SYMLINK && i < components.len() - 1 {
                let link_inode = self.read_inode_from_blkno(found.inode_blkno)?;
                let target = self.read_symlink_target(&link_inode)?;
                let remaining = components[i + 1..].join("/");
                let full_target = if target.starts_with('/') {
                    self.resolve_path_depth(&target, depth + 1)?
                } else {
                    // Relative: resolve from parent directory.
                    let parent_path = components[..i].join("/");
                    let parent_blkno = if parent_path.is_empty() {
                        self.superblock.root_blkno
                    } else {
                        self.resolve_path_depth(&parent_path, depth + 1)?
                    };
                    let combined = format!("{}/{}", parent_path, target);
                    let _ = parent_blkno;
                    return self.resolve_path_depth(&combined, depth + 1);
                };
                let combined = format!("{}/{}", full_target, remaining);
                return self.resolve_path_depth(&combined, depth + 1);
            }

            current_blkno = found.inode_blkno;
        }

        // Check if the final component is a symlink.
        let final_inode = self.read_inode_from_blkno(current_blkno)?;
        if (final_inode.mode & S_IFMT) == S_IFLNK && depth < MAX_SYMLINK_DEPTH {
            let target = self.read_symlink_target(&final_inode)?;
            if target.starts_with('/') {
                return self.resolve_path_depth(&target, depth + 1);
            } else {
                let parent_path = path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
                let combined = if parent_path.is_empty() {
                    target
                } else {
                    format!("{}/{}", parent_path, target)
                };
                return self.resolve_path_depth(&combined, depth + 1);
            }
        }

        Ok(current_blkno)
    }

    /// Read the target of a symlink from an inode's inline data.
    fn read_symlink_target(&self, inode: &Ocfs2Inode) -> FsResult<String> {
        // Symlink target is stored inline in the data array.
        let mut target_bytes = Vec::new();
        for &val in &inode.data {
            if val == 0 {
                break;
            }
            target_bytes.extend_from_slice(&val.to_le_bytes());
        }
        // Trim null bytes.
        let trimmed: Vec<u8> = target_bytes.iter().take_while(|&&b| b != 0).copied().collect();
        alloc::str::from_utf8(&trimmed)
            .map(|s| s.to_string())
            .map_err(|_| FsError::IoError)
    }

    /// Split a path into (parent_path, filename).
    fn split_path<'a>(&self, path: &'a str) -> FsResult<(&'a str, &'a str)> {
        let path = path.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        match path.rfind('/') {
            Some(pos) => Ok((&path[..pos], &path[pos + 1..])),
            None => Ok(("", path)),
        }
    }

    /// Allocate a new block on disk (simplified single-node allocator).
    fn alloc_block(&self) -> FsResult<u64> {
        let mut next = self.next_free_block.write();
        let blkno = *next;
        *next += 1;

        // Zero the new block on disk.
        let block_size = self.superblock.block_size as usize;
        let zero_block = vec![0u8; block_size];
        self.write_block(blkno, &zero_block)?;

        Ok(blkno)
    }

    /// Allocate a new inode (block) and initialize it.
    fn alloc_inode(&self, mode: u16, permissions: FilePermissions) -> FsResult<Ocfs2Inode> {
        let blkno = self.alloc_block()?;
        let now = get_current_time();
        let full_mode = (mode & S_IFMT) | (permissions.to_octal() & 0o777);

        let inode = Ocfs2Inode {
            blkno,
            size: 0,
            mode: full_mode,
            uid: 0,
            gid: 0,
            links_count: 1,
            atime: now,
            ctime: now,
            mtime: now,
            data: [0u32; OCFS2_INLINE_DATA_ENTRIES],
            entries: Vec::new(),
        };

        self.write_inode(&inode)?;
        Ok(inode)
    }

    /// Add a directory entry to a parent directory inode.
    fn add_dir_entry(
        &self,
        parent_blkno: u64,
        name: &str,
        child_blkno: u64,
        file_type: u8,
    ) -> FsResult<()> {
        let mut parent = self.read_inode_from_blkno(parent_blkno)?;

        // Check for duplicate.
        if parent.entries.iter().any(|e| e.name == name) {
            return Err(FsError::AlreadyExists);
        }

        parent.entries.push(Ocfs2DirEntry {
            inode_blkno: child_blkno,
            name: name.to_string(),
            file_type,
        });

        // Update directory size (simplified: count entries * 16).
        parent.size = parent.entries.len() as u64 * 16;
        parent.mtime = get_current_time();

        self.write_inode(&parent)?;
        Ok(())
    }

    /// Remove a directory entry from a parent directory inode.
    fn remove_dir_entry(&self, parent_blkno: u64, name: &str) -> FsResult<()> {
        let mut parent = self.read_inode_from_blkno(parent_blkno)?;

        let initial_len = parent.entries.len();
        parent.entries.retain(|e| e.name != name);
        if parent.entries.len() == initial_len {
            return Err(FsError::NotFound);
        }

        parent.size = parent.entries.len() as u64 * 16;
        parent.mtime = get_current_time();

        self.write_inode(&parent)?;
        Ok(())
    }

    /// Convert an OCFS2 inode to FileMetadata.
    fn inode_to_metadata(&self, blkno: u64, inode: &Ocfs2Inode) -> FileMetadata {
        let file_type = mode_to_file_type(inode.mode);
        FileMetadata {
            inode: blkno,
            file_type,
            size: inode.size,
            permissions: FilePermissions::from_octal(inode.mode & 0o777),
            uid: inode.uid,
            gid: inode.gid,
            created: inode.ctime,
            modified: inode.mtime,
            accessed: inode.atime,
            link_count: inode.links_count as u32,
            device_id: None,
        }
    }

    /// Read file data from an inode's inline data blocks.
    fn read_file_data(&self, inode: &Ocfs2Inode, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        if offset >= inode.size {
            return Ok(0);
        }

        let bytes_to_read = core::cmp::min(buffer.len(), (inode.size - offset) as usize);
        let block_size = self.superblock.block_size as u64;
        let mut bytes_read = 0;

        let start_block = offset / block_size;
        let start_offset = (offset % block_size) as usize;

        for block_idx in start_block.. {
            if bytes_read >= bytes_to_read {
                break;
            }

            // Get block number from inline data array.
            let block_num = if (block_idx as usize) < OCFS2_INLINE_DATA_ENTRIES {
                inode.data[block_idx as usize] as u64
            } else {
                0 // No indirect block support in this simplified driver
            };

            let copy_offset = if block_idx == start_block {
                start_offset
            } else {
                0
            };
            let copy_len = core::cmp::min(
                block_size as usize - copy_offset,
                bytes_to_read - bytes_read,
            );

            if block_num == 0 {
                buffer[bytes_read..bytes_read + copy_len].fill(0);
            } else {
                let block_data = self.read_block(block_num)?;
                buffer[bytes_read..bytes_read + copy_len]
                    .copy_from_slice(&block_data[copy_offset..copy_offset + copy_len]);
            }

            bytes_read += copy_len;
        }

        Ok(bytes_read)
    }

    /// Write data to an inode, allocating blocks as needed.
    fn write_file_data(&self, inode_blkno: u64, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }

        let mut inode = self.read_inode_from_blkno(inode_blkno)?;
        let block_size = self.superblock.block_size as u64;

        let max_size = (OCFS2_INLINE_DATA_ENTRIES as u64) * block_size;
        if offset >= max_size {
            return Err(FsError::NoSpaceLeft);
        }

        let end = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        let writable_end = core::cmp::min(end, max_size);
        let writable_len = (writable_end - offset) as usize;
        let writable = &buffer[..writable_len];

        let mut bytes_written = 0usize;
        let mut block_idx = offset / block_size;
        let mut block_off = (offset % block_size) as usize;

        while bytes_written < writable_len {
            let block_num = if (block_idx as usize) < OCFS2_INLINE_DATA_ENTRIES {
                inode.data[block_idx as usize] as u64
            } else {
                return Err(FsError::NoSpaceLeft);
            };

            let actual_block = if block_num == 0 {
                // Allocate a new block.
                let new_blk = self.alloc_block()?;
                inode.data[block_idx as usize] = new_blk as u32;
                new_blk
            } else {
                block_num
            };

            let mut data = self.read_block(actual_block)?;
            let copy_len = core::cmp::min(
                block_size as usize - block_off,
                writable_len - bytes_written,
            );
            data[block_off..block_off + copy_len]
                .copy_from_slice(&writable[bytes_written..bytes_written + copy_len]);
            self.write_block(actual_block, &data)?;

            bytes_written += copy_len;
            block_idx += 1;
            block_off = 0;
        }

        // Update inode size and timestamps.
        let new_size = core::cmp::max(inode.size, offset + bytes_written as u64);
        inode.size = new_size;
        inode.mtime = get_current_time();
        inode.atime = inode.mtime;

        self.write_inode(&inode)?;
        Ok(bytes_written)
    }
}

impl FileSystem for Ocfs2FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::RamFs // OCFS2 not in FileSystemType enum; closest is RamFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        Ok(FileSystemStats {
            total_blocks: self.superblock.total_clusters as u64,
            free_blocks: self.superblock.free_clusters as u64,
            available_blocks: self.superblock.free_clusters as u64,
            total_inodes: 0, // OCFS2 uses dynamic inode allocation
            free_inodes: 0,
            block_size: self.superblock.block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, filename) = self.split_path(path)?;

        let parent_blkno = self.resolve_path(parent_path)?;
        let parent_inode = self.read_inode_from_blkno(parent_blkno)?;
        if (parent_inode.mode & S_IFMT) != S_IFDIR {
            return Err(FsError::NotADirectory);
        }

        // Check if file already exists.
        if parent_inode.entries.iter().any(|e| e.name == filename) {
            return Err(FsError::AlreadyExists);
        }

        let new_inode = self.alloc_inode(S_IFREG, permissions)?;
        self.add_dir_entry(parent_blkno, filename, new_inode.blkno, OCFS2_FT_REG_FILE)?;

        Ok(new_inode.blkno)
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        match self.resolve_path(path) {
            Ok(blkno) => {
                if flags.truncate && flags.write {
                    let mut inode = self.read_inode_from_blkno(blkno)?;
                    if (inode.mode & S_IFMT) == S_IFDIR {
                        return Err(FsError::IsADirectory);
                    }
                    inode.size = 0;
                    inode.data = [0u32; OCFS2_INLINE_DATA_ENTRIES];
                    inode.mtime = get_current_time();
                    self.write_inode(&inode)?;
                }
                Ok(blkno)
            }
            Err(FsError::NotFound) if flags.create => {
                self.create(path, FilePermissions::default_file())
            }
            Err(e) => Err(e),
        }
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let blkno = inode;
        let ocfs2_inode = self.read_inode_from_blkno(blkno)?;

        if (ocfs2_inode.mode & S_IFMT) == S_IFDIR {
            return Err(FsError::IsADirectory);
        }

        self.read_file_data(&ocfs2_inode, offset, buffer)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let blkno = inode;
        let ocfs2_inode = self.read_inode_from_blkno(blkno)?;

        if (ocfs2_inode.mode & S_IFMT) == S_IFDIR {
            return Err(FsError::IsADirectory);
        }

        self.write_file_data(blkno, offset, buffer)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let blkno = inode;
        let ocfs2_inode = self.read_inode_from_blkno(blkno)?;
        Ok(self.inode_to_metadata(blkno, &ocfs2_inode))
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let blkno = inode;
        let mut ocfs2_inode = self.read_inode_from_blkno(blkno)?;

        ocfs2_inode.mode = (ocfs2_inode.mode & S_IFMT) | (metadata.permissions.to_octal() & 0o777);
        ocfs2_inode.uid = metadata.uid;
        ocfs2_inode.gid = metadata.gid;
        ocfs2_inode.atime = metadata.accessed;
        ocfs2_inode.mtime = metadata.modified;
        ocfs2_inode.ctime = metadata.created;

        self.write_inode(&ocfs2_inode)?;
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, dirname) = self.split_path(path)?;

        let parent_blkno = self.resolve_path(parent_path)?;
        let parent_inode = self.read_inode_from_blkno(parent_blkno)?;
        if (parent_inode.mode & S_IFMT) != S_IFDIR {
            return Err(FsError::NotADirectory);
        }

        if parent_inode.entries.iter().any(|e| e.name == dirname) {
            return Err(FsError::AlreadyExists);
        }

        let new_inode = self.alloc_inode(S_IFDIR, permissions)?;
        // Add "." and ".." entries.
        let mut dir_inode = new_inode.clone();
        dir_inode.entries.push(Ocfs2DirEntry {
            inode_blkno: new_inode.blkno,
            name: ".".to_string(),
            file_type: OCFS2_FT_DIR,
        });
        dir_inode.entries.push(Ocfs2DirEntry {
            inode_blkno: parent_blkno,
            name: "..".to_string(),
            file_type: OCFS2_FT_DIR,
        });
        dir_inode.links_count = 2;
        dir_inode.size = 32;
        self.write_inode(&dir_inode)?;

        self.add_dir_entry(parent_blkno, dirname, new_inode.blkno, OCFS2_FT_DIR)?;

        // Increment parent link count.
        let mut parent = self.read_inode_from_blkno(parent_blkno)?;
        parent.links_count += 1;
        self.write_inode(&parent)?;

        Ok(new_inode.blkno)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let (parent_path, dirname) = self.split_path(path)?;
        let parent_blkno = self.resolve_path(parent_path)?;
        let dir_blkno = self.resolve_path(path)?;
        let dir_inode = self.read_inode_from_blkno(dir_blkno)?;

        if (dir_inode.mode & S_IFMT) != S_IFDIR {
            return Err(FsError::NotADirectory);
        }

        // Check if directory is empty (only "." and "..").
        if dir_inode
            .entries
            .iter()
            .filter(|e| e.name != "." && e.name != "..")
            .count()
            > 0
        {
            return Err(FsError::DirectoryNotEmpty);
        }

        self.remove_dir_entry(parent_blkno, dirname)?;

        // Decrement parent link count.
        let mut parent = self.read_inode_from_blkno(parent_blkno)?;
        if parent.links_count > 1 {
            parent.links_count -= 1;
        }
        self.write_inode(&parent)?;

        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let (parent_path, filename) = self.split_path(path)?;
        let parent_blkno = self.resolve_path(parent_path)?;
        let file_blkno = self.resolve_path(path)?;
        let file_inode = self.read_inode_from_blkno(file_blkno)?;

        if (file_inode.mode & S_IFMT) == S_IFDIR {
            return Err(FsError::IsADirectory);
        }

        self.remove_dir_entry(parent_blkno, filename)?;

        // Decrement link count.
        let mut inode = file_inode;
        if inode.links_count > 1 {
            inode.links_count -= 1;
            self.write_inode(&inode)?;
        }
        // If links_count drops to 0, we would free blocks (simplified: leave for now).

        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let blkno = inode;
        let ocfs2_inode = self.read_inode_from_blkno(blkno)?;

        if (ocfs2_inode.mode & S_IFMT) != S_IFDIR {
            return Err(FsError::NotADirectory);
        }

        let mut entries = Vec::new();
        for entry in &ocfs2_inode.entries {
            if entry.name == "." || entry.name == ".." {
                continue;
            }
            let file_type = match entry.file_type {
                OCFS2_FT_REG_FILE => FileType::Regular,
                OCFS2_FT_DIR => FileType::Directory,
                OCFS2_FT_SYMLINK => FileType::SymbolicLink,
                _ => FileType::Regular,
            };
            entries.push(DirectoryEntry {
                name: entry.name.clone(),
                inode: entry.inode_blkno,
                file_type,
            });
        }
        Ok(entries)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let (old_parent_path, old_name) = self.split_path(old_path)?;
        let (new_parent_path, new_name) = self.split_path(new_path)?;

        let old_parent_blkno = self.resolve_path(old_parent_path)?;
        let new_parent_blkno = self.resolve_path(new_parent_path)?;

        let old_parent = self.read_inode_from_blkno(old_parent_blkno)?;
        let entry = old_parent
            .entries
            .iter()
            .find(|e| e.name == old_name)
            .ok_or(FsError::NotFound)?
            .clone();

        // Check if target already exists.
        let new_parent = self.read_inode_from_blkno(new_parent_blkno)?;
        if new_parent.entries.iter().any(|e| e.name == new_name) {
            return Err(FsError::AlreadyExists);
        }

        // Remove from old parent.
        self.remove_dir_entry(old_parent_blkno, old_name)?;
        // Add to new parent.
        self.add_dir_entry(new_parent_blkno, new_name, entry.inode_blkno, entry.file_type)?;

        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_path, linkname) = self.split_path(link_path)?;

        let parent_blkno = self.resolve_path(parent_path)?;
        let parent_inode = self.read_inode_from_blkno(parent_blkno)?;
        if (parent_inode.mode & S_IFMT) != S_IFDIR {
            return Err(FsError::NotADirectory);
        }

        if parent_inode.entries.iter().any(|e| e.name == linkname) {
            return Err(FsError::AlreadyExists);
        }

        let mut new_inode = self.alloc_inode(S_IFLNK, FilePermissions::default_file())?;

        // Store target in inline data.
        let target_bytes = target.as_bytes();
        let mut data = [0u32; OCFS2_INLINE_DATA_ENTRIES];
        let mut byte_buf = [0u8; OCFS2_INLINE_DATA_ENTRIES * 4];
        let copy_len = core::cmp::min(target_bytes.len(), byte_buf.len());
        byte_buf[..copy_len].copy_from_slice(&target_bytes[..copy_len]);
        for i in 0..OCFS2_INLINE_DATA_ENTRIES {
            data[i] = u32::from_le_bytes([
                byte_buf[i * 4],
                byte_buf[i * 4 + 1],
                byte_buf[i * 4 + 2],
                byte_buf[i * 4 + 3],
            ]);
        }
        new_inode.data = data;
        new_inode.size = target_bytes.len() as u64;
        self.write_inode(&new_inode)?;

        self.add_dir_entry(parent_blkno, linkname, new_inode.blkno, OCFS2_FT_SYMLINK)?;

        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let blkno = self.resolve_path(path)?;
        let inode = self.read_inode_from_blkno(blkno)?;

        if (inode.mode & S_IFMT) != S_IFLNK {
            return Err(FsError::InvalidArgument);
        }

        self.read_symlink_target(&inode)
    }

    fn sync(&self) -> FsResult<()> {
        // In this implementation, all writes go directly to disk via write_block,
        // so there is no separate flush step needed.
        Ok(())
    }
}

// ============================================================================
// Helper functions for reading/writing little-endian values from byte buffers
// ============================================================================

fn read_u16_le(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buf[offset], buf[offset + 1]])
}

fn read_u32_le(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

fn read_u64_le(buf: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
        buf[offset + 4],
        buf[offset + 5],
        buf[offset + 6],
        buf[offset + 7],
    ])
}

fn write_u16_le(buf: &mut [u8], offset: usize, val: u16) {
    buf[offset..offset + 2].copy_from_slice(&val.to_le_bytes());
}

fn write_u32_le(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

fn write_u64_le(buf: &mut [u8], offset: usize, val: u64) {
    buf[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
}

/// Convert an inode mode to a FileType.
fn mode_to_file_type(mode: u16) -> FileType {
    match mode & S_IFMT {
        S_IFREG => FileType::Regular,
        S_IFDIR => FileType::Directory,
        S_IFLNK => FileType::SymbolicLink,
        S_IFCHR => FileType::CharacterDevice,
        S_IFBLK => FileType::BlockDevice,
        S_IFIFO => FileType::NamedPipe,
        S_IFSOCK => FileType::Socket,
        _ => FileType::Regular,
    }
}
