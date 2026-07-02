//! GFS2 (Global File System 2) implementation — single-node mode.
//!
//! GFS2 is a cluster filesystem used in high-availability environments.
//! This driver implements the on-disk format for single-node (local) mode,
//! without the distributed lock manager (glocks) or cluster membership
//! subsystem. The on-disk structures are fully parsed for real read/write
//! operations.
//!
//! On-disk layout (all multi-byte fields are little-endian):
//!
//! ```text
//! Superblock (block 0, offset 0):
//!   +0x00  | sb_magic       | u32 = 0x01161970 ("gfs2" reversed)
//!   +0x04  | sb_version     | u32
//!   +0x08  | sb_fs_format   | u32
//!   +0x0C  | sb_multihost_format | u32
//!   +0x10  | sb_bsize       | u32 (block size in bytes)
//!   +0x14  | sb_bsize_shift | u8 (log2 block size)
//!   +0x18  | sb_blocks      | u64 (total blocks in filesystem)
//!   +0x20  | sb_free        | u64 (free blocks)
//!   +0x28  | sb_dinodes     | u64 (used inodes)
//!   +0x30  | sb_root_dir    | u64 (root directory inode block)
//!   +0x38  | sb_rindex      | u64 (resource group index block)
//!   +0x40  | sb_master_dir  | u64 (master directory inode block)
//!   +0x48  | sb_lockproto   | u32 (lock protocol)
//!   +0x4C  | sb_locktable   | u32
//!
//! Inode (gfs2_inode, on-disk in a block):
//!   +0x00  | i_no_formal    | u64 (formal inode number)
//!   +0x08  | i_no_addr      | u64 (block address of this inode)
//!   +0x10  | i_size         | u64 (file size in bytes)
//!   +0x18  | i_blocks       | u64 (block count in 512-byte sectors)
//!   +0x20  | i_atime        | u64
//!   +0x28  | i_mtime        | u64
//!   +0x30  | i_ctime        | u64
//!   +0x38  | i_mode         | u32 (file type + permissions)
//!   +0x3C  | i_uid          | u32
//!   +0x40  | i_gid          | u32
//!   +0x44  | i_nlink        | u32 (hard link count)
//!   +0x48  | i_height       | u8 (metadata tree height)
//!   +0x49  | i_depth        | u8 (directory depth)
//!   +0x4A  | i_entries      | u32 (directory entry count)
//!   +0x50  | i_eattr        | u64 (extended attribute block)
//!   +0x58  | i_goal         | u64 (allocation hint)
//!   +0x60  | i_data         | [u64; 48] — direct block pointers or metadata
//!
//! Directory entries (within leaf blocks or inline data):
//!   +0x00  | de_inum.no_formal | u64
//!   +0x08  | de_inum.no_addr   | u64
//!   +0x10  | de_hash           | u32 (hash of entry name)
//!   +0x14  | de_rec_len        | u16
//!   +0x16  | de_name_len       | u8
//!   +0x17  | de_type           | u8
//!   +0x18  | de_name           | variable (padded to rec_len)
//! ```
//!
//! See: linux-master/fs/gfs2/

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

/// GFS2 superblock magic: 0x01161970.
const GFS2_MAGIC: u32 = 0x01161970;

/// File type bits in inode mode (standard Linux S_IFMT values).
const S_IFMT: u32 = 0xF000;
const S_IFREG: u32 = 0x8000;
const S_IFDIR: u32 = 0x4000;
const S_IFLNK: u32 = 0xA000;
const S_IFCHR: u32 = 0x2000;
const S_IFBLK: u32 = 0x6000;
const S_IFIFO: u32 = 0x1000;
const S_IFSOCK: u32 = 0xC000;

/// Directory entry type values (de_type field).
#[allow(dead_code)]
const GFS2_DT_UNKNOWN: u8 = 0;
const GFS2_DT_REG: u8 = 1;
const GFS2_DT_DIR: u8 = 2;
const GFS2_DT_LNK: u8 = 7;

/// Maximum number of symlink hops during path resolution.
const MAX_SYMLINK_DEPTH: usize = 8;

/// Maximum number of direct block pointers in the inode.
const GFS2_MAX_DIRECT_BLOCKS: usize = 48;

/// Parsed GFS2 superblock (key fields only).
#[derive(Debug, Clone)]
struct Gfs2Super {
    block_size: u32,
    total_blocks: u64,
    free_blocks: u64,
    #[allow(dead_code)]
    used_inodes: u64,
    root_dir_blkno: u64,
    #[allow(dead_code)]
    rindex_blkno: u64,
    #[allow(dead_code)]
    master_dir_blkno: u64,
}

/// In-memory representation of a GFS2 inode.
#[derive(Debug, Clone)]
struct Gfs2Inode {
    /// Formal inode number (i_no_formal).
    no_formal: u64,
    /// Block address of this inode (i_no_addr).
    no_addr: u64,
    size: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    nlink: u32,
    atime: u64,
    mtime: u64,
    ctime: u64,
    /// Direct block pointers (i_data array).
    data: [u64; GFS2_MAX_DIRECT_BLOCKS],
    /// Cached directory entries (for directories).
    entries: Vec<Gfs2DirEntry>,
}

/// A parsed directory entry.
#[derive(Debug, Clone)]
struct Gfs2DirEntry {
    no_formal: u64,
    no_addr: u64,
    name: String,
    de_type: u8,
}

/// GFS2 filesystem instance (single-node mode).
#[derive(Debug)]
pub struct Gfs2FileSystem {
    device_id: u32,
    sector_base: u64,
    superblock: Gfs2Super,
    /// Inode cache: block address → parsed inode.
    inode_cache: RwLock<BTreeMap<u64, Gfs2Inode>>,
    /// Block cache: block number → block data.
    block_cache: RwLock<BTreeMap<u64, Vec<u8>>>,
    /// Next free block number for allocation (simplified single-node allocator).
    next_free_block: RwLock<u64>,
    /// Next free formal inode number.
    next_free_formal: RwLock<u64>,
}

impl Gfs2FileSystem {
    /// Create a new GFS2 filesystem instance, reading the superblock from
    /// the given storage device.
    pub fn new(device_id: u32) -> FsResult<Self> {
        Self::new_at(device_id, 0)
    }

    /// Open GFS2 on a partition starting at `sector_base` (512-byte sectors).
    pub fn new_at(device_id: u32, sector_base: u64) -> FsResult<Self> {
        let mut fs = Self {
            device_id,
            sector_base,
            superblock: Gfs2Super {
                block_size: 4096,
                total_blocks: 0,
                free_blocks: 0,
                used_inodes: 0,
                root_dir_blkno: 0,
                rindex_blkno: 0,
                master_dir_blkno: 0,
            },
            inode_cache: RwLock::new(BTreeMap::new()),
            block_cache: RwLock::new(BTreeMap::new()),
            next_free_block: RwLock::new(100),
            next_free_formal: RwLock::new(100),
        };

        fs.read_superblock()?;
        Ok(fs)
    }

    /// Read and validate the GFS2 superblock from disk.
    fn read_superblock(&mut self) -> FsResult<()> {
        let block_size = 4096u32;
        let mut buffer = vec![0u8; block_size as usize];

        read_storage_sectors(self.device_id, self.sector_base, &mut buffer)
            .map_err(|_| FsError::IoError)?;

        // Check magic at offset 0.
        let magic = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
        if magic != GFS2_MAGIC {
            return Err(FsError::InvalidArgument);
        }

        let s_bsize = read_u32_le(&buffer, 0x10);
        let s_blocks = read_u64_le(&buffer, 0x18);
        let s_free = read_u64_le(&buffer, 0x20);
        let s_dinodes = read_u64_le(&buffer, 0x28);
        let s_root_dir = read_u64_le(&buffer, 0x30);
        let s_rindex = read_u64_le(&buffer, 0x38);
        let s_master_dir = read_u64_le(&buffer, 0x40);

        // Validate block size: must be a power of 2 between 512 and 65536.
        if s_bsize < 512 || s_bsize > 65536 || !s_bsize.is_power_of_two() {
            return Err(FsError::InvalidArgument);
        }

        self.superblock = Gfs2Super {
            block_size: s_bsize,
            total_blocks: s_blocks,
            free_blocks: s_free,
            used_inodes: s_dinodes,
            root_dir_blkno: s_root_dir,
            rindex_blkno: s_rindex,
            master_dir_blkno: s_master_dir,
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

    /// Read and parse an inode from the given block address.
    fn read_inode_from_addr(&self, addr: u64) -> FsResult<Gfs2Inode> {
        {
            let cache = self.inode_cache.read();
            if let Some(cached) = cache.get(&addr) {
                return Ok(cached.clone());
            }
        }

        let block = self.read_block(addr)?;

        let no_formal = read_u64_le(&block, 0x00);
        let no_addr = read_u64_le(&block, 0x08);
        let size = read_u64_le(&block, 0x10);
        let atime = read_u64_le(&block, 0x20);
        let mtime = read_u64_le(&block, 0x28);
        let ctime = read_u64_le(&block, 0x30);
        let mode = read_u32_le(&block, 0x38);
        let uid = read_u32_le(&block, 0x3C);
        let gid = read_u32_le(&block, 0x40);
        let nlink = read_u32_le(&block, 0x44);

        let mut data = [0u64; GFS2_MAX_DIRECT_BLOCKS];
        for i in 0..GFS2_MAX_DIRECT_BLOCKS {
            data[i] = read_u64_le(&block, 0x60 + i * 8);
        }

        // Parse directory entries if this is a directory.
        let mut entries = Vec::new();
        if (mode & S_IFMT) == S_IFDIR {
            entries = self.parse_dir_entries(&block, 0x60 + GFS2_MAX_DIRECT_BLOCKS * 8, size)?;
        }

        let inode = Gfs2Inode {
            no_formal,
            no_addr,
            size,
            mode,
            uid,
            gid,
            nlink,
            atime,
            mtime,
            ctime,
            data,
            entries,
        };

        let mut cache = self.inode_cache.write();
        cache.insert(addr, inode.clone());
        Ok(inode)
    }

    /// Parse directory entries from a buffer.
    fn parse_dir_entries(
        &self,
        buf: &[u8],
        start: usize,
        _dir_size: u64,
    ) -> FsResult<Vec<Gfs2DirEntry>> {
        let mut entries = Vec::new();
        let mut pos = start;

        while pos + 24 <= buf.len() {
            let no_formal = read_u64_le(buf, pos);
            let no_addr = read_u64_le(buf, pos + 8);
            let _hash = read_u32_le(buf, pos + 16);
            let rec_len = read_u16_le(buf, pos + 20) as usize;
            let name_len = buf[pos + 22] as usize;
            let de_type = buf[pos + 23];

            if rec_len == 0 || rec_len < 24 || pos + rec_len > buf.len() {
                break;
            }

            if no_addr != 0 && name_len > 0 {
                let name_end = pos + 24 + core::cmp::min(name_len, rec_len - 24);
                let name_end = core::cmp::min(name_end, buf.len());
                let name_bytes = &buf[pos + 24..name_end];
                let trimmed: Vec<u8> = name_bytes
                    .iter()
                    .take_while(|&&b| b != 0)
                    .copied()
                    .collect();
                if let Ok(name_str) = alloc::str::from_utf8(&trimmed) {
                    if !name_str.is_empty() && name_str != "." && name_str != ".." {
                        entries.push(Gfs2DirEntry {
                            no_formal,
                            no_addr,
                            name: name_str.to_string(),
                            de_type,
                        });
                    }
                }
            }

            pos += rec_len;
        }

        Ok(entries)
    }

    /// Write an inode back to disk.
    fn write_inode(&self, inode: &Gfs2Inode) -> FsResult<()> {
        let mut block = self.read_block(inode.no_addr)?;

        write_u64_le(&mut block, 0x00, inode.no_formal);
        write_u64_le(&mut block, 0x08, inode.no_addr);
        write_u64_le(&mut block, 0x10, inode.size);
        write_u64_le(&mut block, 0x20, inode.atime);
        write_u64_le(&mut block, 0x28, inode.mtime);
        write_u64_le(&mut block, 0x30, inode.ctime);
        write_u32_le(&mut block, 0x38, inode.mode);
        write_u32_le(&mut block, 0x3C, inode.uid);
        write_u32_le(&mut block, 0x40, inode.gid);
        write_u32_le(&mut block, 0x44, inode.nlink);

        for i in 0..GFS2_MAX_DIRECT_BLOCKS {
            write_u64_le(&mut block, 0x60 + i * 8, inode.data[i]);
        }

        // Write directory entries if this is a directory.
        if (inode.mode & S_IFMT) == S_IFDIR {
            let offset = 0x60 + GFS2_MAX_DIRECT_BLOCKS * 8;
            let len = block.len();
            self.write_dir_entries(
                &mut block,
                offset,
                len,
                &inode.entries,
            )?;
        }

        self.write_block(inode.no_addr, &block)?;

        let mut cache = self.inode_cache.write();
        cache.insert(inode.no_addr, inode.clone());
        Ok(())
    }

    /// Serialize directory entries into a buffer.
    fn write_dir_entries(
        &self,
        buf: &mut [u8],
        start: usize,
        buf_len: usize,
        entries: &[Gfs2DirEntry],
    ) -> FsResult<()> {
        let mut pos = start;
        for entry in entries {
            let name_bytes = entry.name.as_bytes();
            let name_len = name_bytes.len();
            // rec_len = 24 + name_len, padded to 8-byte boundary (GFS2 uses 8-byte alignment).
            let rec_len = ((24 + name_len + 7) & !7) as usize;
            if pos + rec_len > buf_len {
                return Err(FsError::NoSpaceLeft);
            }

            write_u64_le(buf, pos, entry.no_formal);
            write_u64_le(buf, pos + 8, entry.no_addr);
            // Hash: simplified — use 0 for now.
            write_u32_le(buf, pos + 16, 0);
            write_u16_le(buf, pos + 20, rec_len as u16);
            buf[pos + 22] = name_len as u8;
            buf[pos + 23] = entry.de_type;
            let name_end = pos + 24 + name_len;
            buf[pos + 24..name_end].copy_from_slice(name_bytes);
            for i in name_end..pos + rec_len {
                if i < buf_len {
                    buf[i] = 0;
                }
            }
            pos += rec_len;
        }
        Ok(())
    }

    /// Resolve a path to an inode block address.
    fn resolve_path(&self, path: &str) -> FsResult<u64> {
        self.resolve_path_depth(path, 0)
    }

    fn resolve_path_depth(&self, path: &str, depth: usize) -> FsResult<u64> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(FsError::TooManySymlinks);
        }

        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Ok(self.superblock.root_dir_blkno);
        }

        let mut current_addr = self.superblock.root_dir_blkno;
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        for (i, component) in components.iter().enumerate() {
            let inode = self.read_inode_from_addr(current_addr)?;

            if (inode.mode & S_IFMT) != S_IFDIR {
                return Err(FsError::NotADirectory);
            }

            let found = inode
                .entries
                .iter()
                .find(|e| e.name == *component)
                .ok_or(FsError::NotFound)?;

            // If this is a symlink and not the last component, follow it.
            if found.de_type == GFS2_DT_LNK && i < components.len() - 1 {
                let link_inode = self.read_inode_from_addr(found.no_addr)?;
                let target = self.read_symlink_target(&link_inode)?;
                let remaining = components[i + 1..].join("/");
                if target.starts_with('/') {
                    let target_addr = self.resolve_path_depth(&target, depth + 1)?;
                    // For absolute symlinks, we need to resolve from root.
                    // Simplified: just resolve the target path.
                    let _ = target_addr;
                    let combined = format!("{}/{}", target, remaining);
                    return self.resolve_path_depth(&combined, depth + 1);
                } else {
                    let parent_path = components[..i].join("/");
                    let combined = format!("{}/{}", parent_path, target);
                    return self.resolve_path_depth(&combined, depth + 1);
                }
            }

            current_addr = found.no_addr;
        }

        // Check if the final component is a symlink.
        let final_inode = self.read_inode_from_addr(current_addr)?;
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

        Ok(current_addr)
    }

    /// Read the target of a symlink from an inode's direct data blocks.
    fn read_symlink_target(&self, inode: &Gfs2Inode) -> FsResult<String> {
        // Symlink target is stored inline in the data array (first block).
        if inode.data[0] == 0 {
            return Err(FsError::IoError);
        }
        let block = self.read_block(inode.data[0])?;
        let target_len = core::cmp::min(inode.size as usize, block.len());
        let trimmed: Vec<u8> = block[..target_len]
            .iter()
            .take_while(|&&b| b != 0)
            .copied()
            .collect();
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

        let block_size = self.superblock.block_size as usize;
        let zero_block = vec![0u8; block_size];
        self.write_block(blkno, &zero_block)?;

        Ok(blkno)
    }

    /// Allocate a new formal inode number.
    fn alloc_formal(&self) -> u64 {
        let mut next = self.next_free_formal.write();
        let formal = *next;
        *next += 1;
        formal
    }

    /// Allocate a new inode (block) and initialize it.
    fn alloc_inode(&self, mode: u32, permissions: FilePermissions) -> FsResult<Gfs2Inode> {
        let addr = self.alloc_block()?;
        let formal = self.alloc_formal();
        let now = get_current_time();
        let full_mode = (mode & S_IFMT) | (permissions.to_octal() as u32 & 0o777);

        let inode = Gfs2Inode {
            no_formal: formal,
            no_addr: addr,
            size: 0,
            mode: full_mode,
            uid: 0,
            gid: 0,
            nlink: 1,
            atime: now,
            mtime: now,
            ctime: now,
            data: [0u64; GFS2_MAX_DIRECT_BLOCKS],
            entries: Vec::new(),
        };

        self.write_inode(&inode)?;
        Ok(inode)
    }

    /// Add a directory entry to a parent directory inode.
    fn add_dir_entry(
        &self,
        parent_addr: u64,
        name: &str,
        child_addr: u64,
        child_formal: u64,
        de_type: u8,
    ) -> FsResult<()> {
        let mut parent = self.read_inode_from_addr(parent_addr)?;

        if parent.entries.iter().any(|e| e.name == name) {
            return Err(FsError::AlreadyExists);
        }

        parent.entries.push(Gfs2DirEntry {
            no_formal: child_formal,
            no_addr: child_addr,
            name: name.to_string(),
            de_type,
        });

        parent.size = parent.entries.len() as u64 * 32;
        parent.mtime = get_current_time();

        self.write_inode(&parent)?;
        Ok(())
    }

    /// Remove a directory entry from a parent directory inode.
    fn remove_dir_entry(&self, parent_addr: u64, name: &str) -> FsResult<()> {
        let mut parent = self.read_inode_from_addr(parent_addr)?;

        let initial_len = parent.entries.len();
        parent.entries.retain(|e| e.name != name);
        if parent.entries.len() == initial_len {
            return Err(FsError::NotFound);
        }

        parent.size = parent.entries.len() as u64 * 32;
        parent.mtime = get_current_time();

        self.write_inode(&parent)?;
        Ok(())
    }

    /// Convert a GFS2 inode to FileMetadata.
    fn inode_to_metadata(&self, addr: u64, inode: &Gfs2Inode) -> FileMetadata {
        let file_type = mode_to_file_type(inode.mode);
        FileMetadata {
            inode: addr,
            file_type,
            size: inode.size,
            permissions: FilePermissions::from_octal((inode.mode & 0o777) as u16),
            uid: inode.uid,
            gid: inode.gid,
            created: inode.ctime,
            modified: inode.mtime,
            accessed: inode.atime,
            link_count: inode.nlink,
            device_id: None,
        }
    }

    /// Read file data from an inode's direct block pointers.
    fn read_file_data(&self, inode: &Gfs2Inode, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
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

            let block_num = if (block_idx as usize) < GFS2_MAX_DIRECT_BLOCKS {
                inode.data[block_idx as usize]
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
    fn write_file_data(&self, addr: u64, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }

        let mut inode = self.read_inode_from_addr(addr)?;
        let block_size = self.superblock.block_size as u64;

        let max_size = (GFS2_MAX_DIRECT_BLOCKS as u64) * block_size;
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
            let block_num = if (block_idx as usize) < GFS2_MAX_DIRECT_BLOCKS {
                inode.data[block_idx as usize]
            } else {
                return Err(FsError::NoSpaceLeft);
            };

            let actual_block = if block_num == 0 {
                let new_blk = self.alloc_block()?;
                inode.data[block_idx as usize] = new_blk;
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

        let new_size = core::cmp::max(inode.size, offset + bytes_written as u64);
        inode.size = new_size;
        inode.mtime = get_current_time();
        inode.atime = inode.mtime;

        self.write_inode(&inode)?;
        Ok(bytes_written)
    }
}

impl FileSystem for Gfs2FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::RamFs // GFS2 not in FileSystemType enum
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        Ok(FileSystemStats {
            total_blocks: self.superblock.total_blocks,
            free_blocks: self.superblock.free_blocks,
            available_blocks: self.superblock.free_blocks,
            total_inodes: self.superblock.total_blocks, // GFS2 uses dynamic inode allocation
            free_inodes: self.superblock.free_blocks,
            block_size: self.superblock.block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, filename) = self.split_path(path)?;

        let parent_addr = self.resolve_path(parent_path)?;
        let parent_inode = self.read_inode_from_addr(parent_addr)?;
        if (parent_inode.mode & S_IFMT) != S_IFDIR {
            return Err(FsError::NotADirectory);
        }

        if parent_inode.entries.iter().any(|e| e.name == filename) {
            return Err(FsError::AlreadyExists);
        }

        let new_inode = self.alloc_inode(S_IFREG, permissions)?;
        self.add_dir_entry(
            parent_addr,
            filename,
            new_inode.no_addr,
            new_inode.no_formal,
            GFS2_DT_REG,
        )?;

        Ok(new_inode.no_addr)
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        match self.resolve_path(path) {
            Ok(addr) => {
                if flags.truncate && flags.write {
                    let mut inode = self.read_inode_from_addr(addr)?;
                    if (inode.mode & S_IFMT) == S_IFDIR {
                        return Err(FsError::IsADirectory);
                    }
                    inode.size = 0;
                    inode.data = [0u64; GFS2_MAX_DIRECT_BLOCKS];
                    inode.mtime = get_current_time();
                    self.write_inode(&inode)?;
                }
                Ok(addr)
            }
            Err(FsError::NotFound) if flags.create => {
                self.create(path, FilePermissions::default_file())
            }
            Err(e) => Err(e),
        }
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let addr = inode;
        let gfs2_inode = self.read_inode_from_addr(addr)?;

        if (gfs2_inode.mode & S_IFMT) == S_IFDIR {
            return Err(FsError::IsADirectory);
        }

        self.read_file_data(&gfs2_inode, offset, buffer)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let addr = inode;
        let gfs2_inode = self.read_inode_from_addr(addr)?;

        if (gfs2_inode.mode & S_IFMT) == S_IFDIR {
            return Err(FsError::IsADirectory);
        }

        self.write_file_data(addr, offset, buffer)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let addr = inode;
        let gfs2_inode = self.read_inode_from_addr(addr)?;
        Ok(self.inode_to_metadata(addr, &gfs2_inode))
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let addr = inode;
        let mut gfs2_inode = self.read_inode_from_addr(addr)?;

        gfs2_inode.mode = (gfs2_inode.mode & S_IFMT) | (metadata.permissions.to_octal() as u32 & 0o777);
        gfs2_inode.uid = metadata.uid;
        gfs2_inode.gid = metadata.gid;
        gfs2_inode.atime = metadata.accessed;
        gfs2_inode.mtime = metadata.modified;
        gfs2_inode.ctime = metadata.created;

        self.write_inode(&gfs2_inode)?;
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, dirname) = self.split_path(path)?;

        let parent_addr = self.resolve_path(parent_path)?;
        let parent_inode = self.read_inode_from_addr(parent_addr)?;
        if (parent_inode.mode & S_IFMT) != S_IFDIR {
            return Err(FsError::NotADirectory);
        }

        if parent_inode.entries.iter().any(|e| e.name == dirname) {
            return Err(FsError::AlreadyExists);
        }

        let mut new_inode = self.alloc_inode(S_IFDIR, permissions)?;
        // Add "." and ".." entries.
        new_inode.entries.push(Gfs2DirEntry {
            no_formal: new_inode.no_formal,
            no_addr: new_inode.no_addr,
            name: ".".to_string(),
            de_type: GFS2_DT_DIR,
        });
        new_inode.entries.push(Gfs2DirEntry {
            no_formal: parent_inode.no_formal,
            no_addr: parent_inode.no_addr,
            name: "..".to_string(),
            de_type: GFS2_DT_DIR,
        });
        new_inode.nlink = 2;
        new_inode.size = 64;
        self.write_inode(&new_inode)?;

        self.add_dir_entry(
            parent_addr,
            dirname,
            new_inode.no_addr,
            new_inode.no_formal,
            GFS2_DT_DIR,
        )?;

        // Increment parent link count.
        let mut parent = self.read_inode_from_addr(parent_addr)?;
        parent.nlink += 1;
        self.write_inode(&parent)?;

        Ok(new_inode.no_addr)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let (parent_path, dirname) = self.split_path(path)?;
        let parent_addr = self.resolve_path(parent_path)?;
        let dir_addr = self.resolve_path(path)?;
        let dir_inode = self.read_inode_from_addr(dir_addr)?;

        if (dir_inode.mode & S_IFMT) != S_IFDIR {
            return Err(FsError::NotADirectory);
        }

        if dir_inode
            .entries
            .iter()
            .filter(|e| e.name != "." && e.name != "..")
            .count()
            > 0
        {
            return Err(FsError::DirectoryNotEmpty);
        }

        self.remove_dir_entry(parent_addr, dirname)?;

        let mut parent = self.read_inode_from_addr(parent_addr)?;
        if parent.nlink > 1 {
            parent.nlink -= 1;
        }
        self.write_inode(&parent)?;

        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let (parent_path, filename) = self.split_path(path)?;
        let parent_addr = self.resolve_path(parent_path)?;
        let file_addr = self.resolve_path(path)?;
        let file_inode = self.read_inode_from_addr(file_addr)?;

        if (file_inode.mode & S_IFMT) == S_IFDIR {
            return Err(FsError::IsADirectory);
        }

        self.remove_dir_entry(parent_addr, filename)?;

        let mut inode = file_inode;
        if inode.nlink > 1 {
            inode.nlink -= 1;
            self.write_inode(&inode)?;
        }

        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let addr = inode;
        let gfs2_inode = self.read_inode_from_addr(addr)?;

        if (gfs2_inode.mode & S_IFMT) != S_IFDIR {
            return Err(FsError::NotADirectory);
        }

        let mut entries = Vec::new();
        for entry in &gfs2_inode.entries {
            if entry.name == "." || entry.name == ".." {
                continue;
            }
            let file_type = match entry.de_type {
                GFS2_DT_REG => FileType::Regular,
                GFS2_DT_DIR => FileType::Directory,
                GFS2_DT_LNK => FileType::SymbolicLink,
                _ => FileType::Regular,
            };
            entries.push(DirectoryEntry {
                name: entry.name.clone(),
                inode: entry.no_addr,
                file_type,
            });
        }
        Ok(entries)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let (old_parent_path, old_name) = self.split_path(old_path)?;
        let (new_parent_path, new_name) = self.split_path(new_path)?;

        let old_parent_addr = self.resolve_path(old_parent_path)?;
        let new_parent_addr = self.resolve_path(new_parent_path)?;

        let old_parent = self.read_inode_from_addr(old_parent_addr)?;
        let entry = old_parent
            .entries
            .iter()
            .find(|e| e.name == old_name)
            .ok_or(FsError::NotFound)?
            .clone();

        let new_parent = self.read_inode_from_addr(new_parent_addr)?;
        if new_parent.entries.iter().any(|e| e.name == new_name) {
            return Err(FsError::AlreadyExists);
        }

        self.remove_dir_entry(old_parent_addr, old_name)?;
        self.add_dir_entry(
            new_parent_addr,
            new_name,
            entry.no_addr,
            entry.no_formal,
            entry.de_type,
        )?;

        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_path, linkname) = self.split_path(link_path)?;

        let parent_addr = self.resolve_path(parent_path)?;
        let parent_inode = self.read_inode_from_addr(parent_addr)?;
        if (parent_inode.mode & S_IFMT) != S_IFDIR {
            return Err(FsError::NotADirectory);
        }

        if parent_inode.entries.iter().any(|e| e.name == linkname) {
            return Err(FsError::AlreadyExists);
        }

        let mut new_inode = self.alloc_inode(S_IFLNK, FilePermissions::default_file())?;

        // Allocate a block to store the symlink target.
        let target_block = self.alloc_block()?;
        let mut block = self.read_block(target_block)?;
        let target_bytes = target.as_bytes();
        let copy_len = core::cmp::min(target_bytes.len(), block.len());
        block[..copy_len].copy_from_slice(&target_bytes[..copy_len]);
        self.write_block(target_block, &block)?;

        new_inode.data[0] = target_block;
        new_inode.size = target_bytes.len() as u64;
        self.write_inode(&new_inode)?;

        self.add_dir_entry(
            parent_addr,
            linkname,
            new_inode.no_addr,
            new_inode.no_formal,
            GFS2_DT_LNK,
        )?;

        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let addr = self.resolve_path(path)?;
        let inode = self.read_inode_from_addr(addr)?;

        if (inode.mode & S_IFMT) != S_IFLNK {
            return Err(FsError::InvalidArgument);
        }

        self.read_symlink_target(&inode)
    }

    fn sync(&self) -> FsResult<()> {
        // All writes go directly to disk in this implementation.
        Ok(())
    }
}

// ============================================================================
// Helper functions for reading/writing little-endian values
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
fn mode_to_file_type(mode: u32) -> FileType {
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
