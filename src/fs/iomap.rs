//! Iomap I/O helper framework implementation
//!
//! This module provides the iomap (I/O mapping) helper framework used by
//! filesystems like ext4 and xfs for block-mapped I/O. It provides:
//! - `Iomap` struct describing a region of file offset -> disk offset mapping.
//! - `IomapOps` trait for filesystem-specific extent lookup.
//! - `BlockDevice` trait for raw block I/O.
//! - `PageCache` for buffered I/O.
//! - `buffered_read` / `buffered_write` / `direct_read` / `direct_write`.
//! - `IomapInode` with extent-based storage.
//! - `IomapFileSystem` — a full `FileSystem` impl that implements `IomapOps`
//!   for itself.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};
use core::cmp;
use spin::{Mutex, RwLock};

// ============================================================================
// Iomap and flags
// ============================================================================

/// Type of an iomap mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IomapType {
    /// Block is mapped to a disk location.
    Mapped,
    /// Block is a hole (no disk allocation).
    Hole,
    /// Block is delayed-allocated (in memory, not yet on disk).
    DelAlloc,
    /// Block is allocated but not yet written (unwritten extent).
    Unwritten,
}

bitflags::bitflags! {
    /// Iomap flags.
    pub struct IomapFlags: u32 {
        /// Write access (allocate blocks as needed).
        const WRITE = 0x01;
        /// Direct I/O (bypass page cache).
        const DIRECT = 0x02;
        /// Do not allocate — report holes.
        const REPORT = 0x04;
        /// Fault in pages.
        const FAULT = 0x08;
    }
}

/// An I/O mapping describing how a file offset range maps to disk.
#[derive(Debug, Clone)]
pub struct Iomap {
    /// File offset (in bytes) where this mapping begins.
    pub offset: u64,
    /// Disk offset (in bytes) where the data resides. 0 for holes.
    pub disk_offset: u64,
    /// Length of this mapping in bytes.
    pub length: u64,
    /// Type of this mapping.
    pub type_: IomapType,
    /// Flags.
    pub flags: IomapFlags,
}

impl Iomap {
    /// Create a hole mapping.
    pub fn hole(offset: u64, length: u64) -> Self {
        Self {
            offset,
            disk_offset: 0,
            length,
            type_: IomapType::Hole,
            flags: IomapFlags::empty(),
        }
    }

    /// Create a mapped mapping.
    pub fn mapped(offset: u64, disk_offset: u64, length: u64) -> Self {
        Self {
            offset,
            disk_offset,
            length,
            type_: IomapType::Mapped,
            flags: IomapFlags::empty(),
        }
    }
}

// ============================================================================
// BlockDevice trait
// ============================================================================

/// Block device abstraction for iomap I/O.
pub trait BlockDevice: Send + Sync {
    fn read_block(&self, block_num: u64, buffer: &mut [u8]) -> FsResult<()>;
    fn write_block(&self, block_num: u64, buffer: &[u8]) -> FsResult<()>;
    fn block_size(&self) -> u32;
    fn num_blocks(&self) -> u64;
}

// ============================================================================
// IomapOps trait
// ============================================================================

/// Trait providing filesystem-specific extent lookup for iomap I/O.
pub trait IomapOps {
    /// Find the mapping for `[offset, offset+count)` in the given inode.
    /// If `flags` contains `WRITE`, blocks should be allocated as needed.
    fn begin(
        &self,
        inode: InodeNumber,
        offset: u64,
        count: u64,
        flags: IomapFlags,
    ) -> FsResult<Iomap>;

    /// Called after I/O is complete on the mapping. Allows the filesystem
    /// to update metadata (e.g., convert unwritten to written).
    fn end(
        &self,
        inode: InodeNumber,
        offset: u64,
        count: u64,
        written: u64,
        iomap: &Iomap,
        flags: IomapFlags,
    ) -> FsResult<()>;
}

// ============================================================================
// PageCache
// ============================================================================

/// Simple page cache keyed by (inode, page_index).
#[derive(Debug)]
pub struct PageCache {
    pages: Mutex<BTreeMap<(InodeNumber, u64), Vec<u8>>>,
    block_size: u32,
}

impl PageCache {
    pub fn new(block_size: u32) -> Self {
        Self {
            pages: Mutex::new(BTreeMap::new()),
            block_size,
        }
    }

    /// Look up a cached page.
    pub fn get(&self, inode: InodeNumber, page_index: u64) -> Option<Vec<u8>> {
        let pages = self.pages.lock();
        pages.get(&(inode, page_index)).cloned()
    }

    /// Insert or update a cached page.
    pub fn insert(&self, inode: InodeNumber, page_index: u64, data: Vec<u8>) {
        let mut pages = self.pages.lock();
        pages.insert((inode, page_index), data);
    }

    /// Remove a cached page.
    pub fn invalidate(&self, inode: InodeNumber, page_index: u64) {
        let mut pages = self.pages.lock();
        pages.remove(&(inode, page_index));
    }

    /// Flush all pages for an inode (no-op in this in-memory implementation
    /// since writes go through the block device directly).
    pub fn flush_inode(&self, _inode: InodeNumber) -> FsResult<()> {
        Ok(())
    }
}

// ============================================================================
// Buffered I/O functions
// ============================================================================

/// Buffered read: loop through ops.begin(), read from block device via the
/// page cache, zero-fill holes.
pub fn buffered_read(
    ops: &dyn IomapOps,
    device: &dyn BlockDevice,
    cache: &PageCache,
    inode: InodeNumber,
    offset: u64,
    buffer: &mut [u8],
) -> FsResult<usize> {
    if buffer.is_empty() {
        return Ok(0);
    }
    let block_size = device.block_size() as u64;
    let mut pos = offset;
    let mut written = 0usize;
    let end = offset + buffer.len() as u64;

    while pos < end {
        let remaining = end - pos;
        let iomap = ops.begin(inode, pos, remaining, IomapFlags::empty())?;
        let map_end = pos + iomap.length;
        let seg_end = cmp::min(map_end, end);
        let seg_len = (seg_end - pos) as usize;

        match iomap.type_ {
            IomapType::Hole | IomapType::DelAlloc => {
                // Zero-fill holes.
                let buf_end = written + seg_len;
                buffer[written..buf_end].fill(0);
            }
            IomapType::Mapped | IomapType::Unwritten => {
                // Read from the block device, using the page cache.
                let mut seg_pos = pos;
                let mut buf_off = written;
                while seg_pos < seg_end {
                    let block_num = iomap.disk_offset / block_size
                        + (seg_pos - iomap.offset) / block_size;
                    let page_index = block_num;
                    let block_off = (seg_pos % block_size) as usize;
                    let chunk = cmp::min(
                        block_size as usize - block_off,
                        (seg_end - seg_pos) as usize,
                    );

                    // Try page cache.
                    let data = if let Some(cached) = cache.get(inode, page_index) {
                        cached
                    } else {
                        let mut block_data = vec![0u8; block_size as usize];
                        device
                            .read_block(block_num, &mut block_data)
                            .map_err(|_| FsError::IoError)?;
                        cache.insert(inode, page_index, block_data.clone());
                        block_data
                    };

                    if iomap.type_ == IomapType::Unwritten {
                        // Unwritten extents read as zeros.
                        buffer[buf_off..buf_off + chunk].fill(0);
                    } else {
                        buffer[buf_off..buf_off + chunk]
                            .copy_from_slice(&data[block_off..block_off + chunk]);
                    }
                    seg_pos += chunk as u64;
                    buf_off += chunk;
                }
            }
        }

        // Call end() to notify the filesystem.
        ops.end(inode, pos, seg_len as u64, seg_len as u64, &iomap, IomapFlags::empty())?;

        written += seg_len;
        pos = seg_end;
    }

    Ok(written)
}

/// Buffered write: loop through ops.begin() with WRITE flag, write to block
/// device via the page cache.
pub fn buffered_write(
    ops: &dyn IomapOps,
    device: &dyn BlockDevice,
    cache: &PageCache,
    inode: InodeNumber,
    offset: u64,
    buffer: &[u8],
) -> FsResult<usize> {
    if buffer.is_empty() {
        return Ok(0);
    }
    let block_size = device.block_size() as u64;
    let mut pos = offset;
    let mut consumed = 0usize;
    let end = offset + buffer.len() as u64;

    while pos < end {
        let remaining = end - pos;
        let iomap = ops.begin(inode, pos, remaining, IomapFlags::WRITE)?;
        let map_end = pos + iomap.length;
        let seg_end = cmp::min(map_end, end);
        let seg_len = (seg_end - pos) as usize;

        match iomap.type_ {
            IomapType::Hole | IomapType::DelAlloc | IomapType::Unwritten => {
                // For holes/delalloc/unwritten, the begin() call should have
                // allocated blocks (since we passed WRITE). If it didn't,
                // we can't write — return an error.
                if iomap.type_ != IomapType::Mapped {
                    return Err(FsError::NoSpaceLeft);
                }
                // Fall through to Mapped handling.
                let mut seg_pos = pos;
                let mut buf_off = consumed;
                while seg_pos < seg_end {
                    let block_num = iomap.disk_offset / block_size
                        + (seg_pos - iomap.offset) / block_size;
                    let page_index = block_num;
                    let block_off = (seg_pos % block_size) as usize;
                    let chunk = cmp::min(
                        block_size as usize - block_off,
                        (seg_end - seg_pos) as usize,
                    );

                    // Read existing block (or cache), modify, write back.
                    let mut data = if let Some(cached) = cache.get(inode, page_index) {
                        cached
                    } else {
                        let mut block_data = vec![0u8; block_size as usize];
                        device
                            .read_block(block_num, &mut block_data)
                            .map_err(|_| FsError::IoError)?;
                        block_data
                    };
                    data[block_off..block_off + chunk]
                        .copy_from_slice(&buffer[buf_off..buf_off + chunk]);
                    device
                        .write_block(block_num, &data)
                        .map_err(|_| FsError::IoError)?;
                    cache.insert(inode, page_index, data);

                    seg_pos += chunk as u64;
                    buf_off += chunk;
                }
            }
            IomapType::Mapped => {
                let mut seg_pos = pos;
                let mut buf_off = consumed;
                while seg_pos < seg_end {
                    let block_num = iomap.disk_offset / block_size
                        + (seg_pos - iomap.offset) / block_size;
                    let page_index = block_num;
                    let block_off = (seg_pos % block_size) as usize;
                    let chunk = cmp::min(
                        block_size as usize - block_off,
                        (seg_end - seg_pos) as usize,
                    );

                    let mut data = if let Some(cached) = cache.get(inode, page_index) {
                        cached
                    } else {
                        let mut block_data = vec![0u8; block_size as usize];
                        device
                            .read_block(block_num, &mut block_data)
                            .map_err(|_| FsError::IoError)?;
                        block_data
                    };
                    data[block_off..block_off + chunk]
                        .copy_from_slice(&buffer[buf_off..buf_off + chunk]);
                    device
                        .write_block(block_num, &data)
                        .map_err(|_| FsError::IoError)?;
                    cache.insert(inode, page_index, data);

                    seg_pos += chunk as u64;
                    buf_off += chunk;
                }
            }
        }

        ops.end(inode, pos, seg_len as u64, seg_len as u64, &iomap, IomapFlags::WRITE)?;

        consumed += seg_len;
        pos = seg_end;
    }

    Ok(consumed)
}

/// Direct read: bypass the page cache, read directly from the block device.
pub fn direct_read(
    ops: &dyn IomapOps,
    device: &dyn BlockDevice,
    inode: InodeNumber,
    offset: u64,
    buffer: &mut [u8],
) -> FsResult<usize> {
    if buffer.is_empty() {
        return Ok(0);
    }
    let block_size = device.block_size() as u64;
    let mut pos = offset;
    let mut written = 0usize;
    let end = offset + buffer.len() as u64;

    while pos < end {
        let remaining = end - pos;
        let iomap = ops.begin(inode, pos, remaining, IomapFlags::DIRECT)?;
        let map_end = pos + iomap.length;
        let seg_end = cmp::min(map_end, end);
        let seg_len = (seg_end - pos) as usize;

        match iomap.type_ {
            IomapType::Hole | IomapType::DelAlloc | IomapType::Unwritten => {
                buffer[written..written + seg_len].fill(0);
            }
            IomapType::Mapped => {
                let mut seg_pos = pos;
                let mut buf_off = written;
                while seg_pos < seg_end {
                    let block_num = iomap.disk_offset / block_size
                        + (seg_pos - iomap.offset) / block_size;
                    let block_off = (seg_pos % block_size) as usize;
                    let chunk = cmp::min(
                        block_size as usize - block_off,
                        (seg_end - seg_pos) as usize,
                    );
                    let mut block_data = vec![0u8; block_size as usize];
                    device
                        .read_block(block_num, &mut block_data)
                        .map_err(|_| FsError::IoError)?;
                    buffer[buf_off..buf_off + chunk]
                        .copy_from_slice(&block_data[block_off..block_off + chunk]);
                    seg_pos += chunk as u64;
                    buf_off += chunk;
                }
            }
        }

        ops.end(inode, pos, seg_len as u64, seg_len as u64, &iomap, IomapFlags::DIRECT)?;
        written += seg_len;
        pos = seg_end;
    }
    Ok(written)
}

/// Direct write: bypass the page cache, write directly to the block device.
pub fn direct_write(
    ops: &dyn IomapOps,
    device: &dyn BlockDevice,
    inode: InodeNumber,
    offset: u64,
    buffer: &[u8],
) -> FsResult<usize> {
    if buffer.is_empty() {
        return Ok(0);
    }
    let block_size = device.block_size() as u64;
    let mut pos = offset;
    let mut consumed = 0usize;
    let end = offset + buffer.len() as u64;

    while pos < end {
        let remaining = end - pos;
        let iomap = ops.begin(inode, pos, remaining, IomapFlags::WRITE | IomapFlags::DIRECT)?;
        let map_end = pos + iomap.length;
        let seg_end = cmp::min(map_end, end);
        let seg_len = (seg_end - pos) as usize;

        if iomap.type_ != IomapType::Mapped {
            return Err(FsError::NoSpaceLeft);
        }

        let mut seg_pos = pos;
        let mut buf_off = consumed;
        while seg_pos < seg_end {
            let block_num = iomap.disk_offset / block_size
                + (seg_pos - iomap.offset) / block_size;
            let block_off = (seg_pos % block_size) as usize;
            let chunk = cmp::min(
                block_size as usize - block_off,
                (seg_end - seg_pos) as usize,
            );
            // For partial block direct writes, read-modify-write.
            if block_off != 0 || chunk != block_size as usize {
                let mut block_data = vec![0u8; block_size as usize];
                device
                    .read_block(block_num, &mut block_data)
                    .map_err(|_| FsError::IoError)?;
                block_data[block_off..block_off + chunk]
                    .copy_from_slice(&buffer[buf_off..buf_off + chunk]);
                device
                    .write_block(block_num, &block_data)
                    .map_err(|_| FsError::IoError)?;
            } else {
                // Full block write — no read needed.
                device
                    .write_block(block_num, &buffer[buf_off..buf_off + chunk])
                    .map_err(|_| FsError::IoError)?;
            }
            seg_pos += chunk as u64;
            buf_off += chunk;
        }

        ops.end(inode, pos, seg_len as u64, seg_len as u64, &iomap, IomapFlags::WRITE | IomapFlags::DIRECT)?;
        consumed += seg_len;
        pos = seg_end;
    }
    Ok(consumed)
}

// ============================================================================
// IomapInode — extent-based storage
// ============================================================================

/// A disk extent: maps a file offset range to a contiguous disk region.
#[derive(Debug, Clone)]
pub struct Extent {
    /// File offset (in bytes) where this extent begins.
    pub file_offset: u64,
    /// Length in bytes.
    pub length: u64,
    /// Starting disk block number.
    pub disk_block: u64,
    /// Whether this extent is unwritten (allocated but not zeroed).
    pub unwritten: bool,
}

/// Inode with extent-based storage for iomap.
#[derive(Debug, Clone)]
pub struct IomapInode {
    pub inode: InodeNumber,
    pub is_dir: bool,
    pub size: u64,
    pub permissions: FilePermissions,
    pub extents: Vec<Extent>,
    pub entries: BTreeMap<String, InodeNumber>,
    pub symlink_target: Option<String>,
    pub link_count: u32,
    pub created: u64,
    pub modified: u64,
    pub accessed: u64,
}

impl IomapInode {
    pub fn new_file(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        Self {
            inode,
            is_dir: false,
            size: 0,
            permissions,
            extents: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: None,
            link_count: 1,
            created: now,
            modified: now,
            accessed: now,
        }
    }

    pub fn new_dir(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        Self {
            inode,
            is_dir: true,
            size: 0,
            permissions,
            extents: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: None,
            link_count: 2,
            created: now,
            modified: now,
            accessed: now,
        }
    }

    pub fn new_symlink(inode: InodeNumber, target: &str) -> Self {
        let now = get_current_time();
        Self {
            inode,
            is_dir: false,
            size: target.len() as u64,
            permissions: FilePermissions::from_octal(0o777),
            extents: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: Some(target.to_string()),
            link_count: 1,
            created: now,
            modified: now,
            accessed: now,
        }
    }

    /// Find the extent covering `offset`, or the hole before the next extent.
    pub fn find_extent(&self, offset: u64) -> Option<&Extent> {
        self.extents.iter().find(|e| {
            offset >= e.file_offset && offset < e.file_offset + e.length
        })
    }

    /// Get the file type.
    pub fn file_type(&self) -> FileType {
        if self.is_dir {
            FileType::Directory
        } else if self.symlink_target.is_some() {
            FileType::SymbolicLink
        } else {
            FileType::Regular
        }
    }

    /// Convert to FileMetadata.
    pub fn to_metadata(&self) -> FileMetadata {
        FileMetadata {
            inode: self.inode,
            file_type: self.file_type(),
            size: self.size,
            permissions: self.permissions,
            uid: 0,
            gid: 0,
            created: self.created,
            modified: self.modified,
            accessed: self.accessed,
            link_count: self.link_count,
            device_id: None,
        }
    }
}

// ============================================================================
// IomapFileSystem
// ============================================================================

/// Iomap-based filesystem with full FileSystem trait implementation.
pub struct IomapFileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, IomapInode>>,
    next_inode: RwLock<InodeNumber>,
    next_block: RwLock<u64>,
    block_device: Arc<dyn BlockDevice>,
    page_cache: PageCache,
    root_inode: InodeNumber,
}

impl core::fmt::Debug for IomapFileSystem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IomapFileSystem")
            .field("root_inode", &self.root_inode)
            .field("next_inode", &*self.next_inode.read())
            .field("next_block", &*self.next_block.read())
            .field("block_size", &self.block_device.block_size())
            .finish()
    }
}

impl IomapFileSystem {
    /// Create a new iomap filesystem backed by `block_device`.
    pub fn new(block_device: Arc<dyn BlockDevice>) -> FsResult<Self> {
        let root = 1u64;
        let mut inodes = BTreeMap::new();
        let mut root_inode = IomapInode::new_dir(root, FilePermissions::default_directory());
        root_inode.entries.insert(".".to_string(), root);
        root_inode.entries.insert("..".to_string(), root);
        inodes.insert(root, root_inode);

        let block_size = block_device.block_size();
        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
            next_block: RwLock::new(1), // Start allocating from block 1
            block_device,
            page_cache: PageCache::new(block_size),
            root_inode: root,
        })
    }

    fn alloc_inode(&self) -> InodeNumber {
        let mut ni = self.next_inode.write();
        let v = *ni;
        *ni += 1;
        v
    }

    fn alloc_block(&self) -> FsResult<u64> {
        let mut nb = self.next_block.write();
        let v = *nb;
        *nb += 1;
        let total = self.block_device.num_blocks();
        if v >= total {
            return Err(FsError::NoSpaceLeft);
        }
        // Zero the block.
        let zero = vec![0u8; self.block_device.block_size() as usize];
        self.block_device
            .write_block(v, &zero)
            .map_err(|_| FsError::IoError)?;
        Ok(v)
    }

    fn split_path(path: &str) -> FsResult<(String, String)> {
        let trimmed = path.trim_end_matches('/');
        if trimmed.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        match trimmed.rfind('/') {
            Some(idx) => {
                let name = trimmed[idx + 1..].to_string();
                if name.is_empty() {
                    return Err(FsError::InvalidArgument);
                }
                let parent = if idx == 0 {
                    String::from("/")
                } else {
                    trimmed[..idx].to_string()
                };
                Ok((parent, name))
            }
            None => Ok((String::from("/"), trimmed.to_string())),
        }
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(self.root_inode);
        }
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        let inodes = self.inodes.read();
        let mut cur = self.root_inode;
        for comp in components {
            let node = inodes.get(&cur).ok_or(FsError::NotFound)?;
            if !node.is_dir {
                return Err(FsError::NotADirectory);
            }
            cur = *node.entries.get(comp).ok_or(FsError::NotFound)?;
        }
        Ok(cur)
    }

    fn get_inode(&self, ino: InodeNumber) -> FsResult<IomapInode> {
        let inodes = self.inodes.read();
        inodes.get(&ino).cloned().ok_or(FsError::NotFound)
    }
}

impl IomapOps for IomapFileSystem {
    fn begin(
        &self,
        inode: InodeNumber,
        offset: u64,
        count: u64,
        flags: IomapFlags,
    ) -> FsResult<Iomap> {
        let node = self.get_inode(inode)?;
        let block_size = self.block_device.block_size() as u64;

        // Find the extent covering this offset.
        if let Some(ext) = node.find_extent(offset) {
            let ext_end = ext.file_offset + ext.length;
            let map_len = cmp::min(ext_end - offset, count);
            let disk_off = ext.disk_block * block_size + (offset - ext.file_offset);
            let type_ = if ext.unwritten {
                IomapType::Unwritten
            } else {
                IomapType::Mapped
            };
            return Ok(Iomap {
                offset,
                disk_offset: disk_off,
                length: map_len,
                type_,
                flags,
            });
        }

        // No extent found — it's a hole or we need to allocate.
        // Find the next extent after this offset to bound the hole.
        let next_ext_start = node
            .extents
            .iter()
            .filter(|e| e.file_offset >= offset)
            .map(|e| e.file_offset)
            .min()
            .unwrap_or(offset + count);

        let hole_len = cmp::min(next_ext_start - offset, count);

        if flags.contains(IomapFlags::WRITE) {
            // Allocate blocks for this region.
            let blocks_needed = (hole_len + block_size - 1) / block_size;
            if blocks_needed == 0 {
                return Ok(Iomap::hole(offset, hole_len));
            }
            let start_block = self.alloc_block()?;
            for _ in 1..blocks_needed {
                let _ = self.alloc_block()?;
            }
            // Add extent to the inode.
            {
                let mut inodes = self.inodes.write();
                if let Some(node) = inodes.get_mut(&inode) {
                    node.extents.push(Extent {
                        file_offset: offset,
                        length: blocks_needed * block_size,
                        disk_block: start_block,
                        unwritten: false,
                    });
                    // Keep extents sorted by file_offset.
                    node.extents.sort_by_key(|e| e.file_offset);
                }
            }
            return Ok(Iomap::mapped(offset, start_block * block_size, hole_len));
        }

        Ok(Iomap::hole(offset, hole_len))
    }

    fn end(
        &self,
        inode: InodeNumber,
        _offset: u64,
        _count: u64,
        _written: u64,
        _iomap: &Iomap,
        _flags: IomapFlags,
    ) -> FsResult<()> {
        // Update access/modification time.
        let mut inodes = self.inodes.write();
        if let Some(node) = inodes.get_mut(&inode) {
            node.accessed = get_current_time();
        }
        Ok(())
    }
}

impl FileSystem for IomapFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::RamFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let total = self.block_device.num_blocks();
        let used = {
            let nb = self.next_block.read();
            *nb
        };
        let block_size = self.block_device.block_size();
        Ok(FileSystemStats {
            total_blocks: total,
            free_blocks: total.saturating_sub(used),
            available_blocks: total.saturating_sub(used),
            total_inodes: u64::MAX,
            free_inodes: u64::MAX - self.inodes.read().len() as u64,
            block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, filename) = Self::split_path(path)?;
        if filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_ino = self.resolve_path(&parent_path)?;
        let new_ino = self.alloc_inode();
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_ino).ok_or(FsError::NotFound)?;
        if !parent.is_dir {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }
        let file_inode = IomapInode::new_file(new_ino, permissions);
        parent.entries.insert(filename, new_ino);
        parent.modified = get_current_time();
        inodes.insert(new_ino, file_inode);
        Ok(new_ino)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let node = self.get_inode(inode)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        if offset >= node.size {
            return Ok(0);
        }
        let to_read = cmp::min(buffer.len(), (node.size - offset) as usize);
        let mut read_buf = vec![0u8; to_read];
        let n = buffered_read(
            self,
            self.block_device.as_ref(),
            &self.page_cache,
            inode,
            offset,
            &mut read_buf,
        )?;
        buffer[..n].copy_from_slice(&read_buf[..n]);
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let node = self.get_inode(inode)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        let n = buffered_write(
            self,
            self.block_device.as_ref(),
            &self.page_cache,
            inode,
            offset,
            buffer,
        )?;
        // Update file size.
        let mut inodes = self.inodes.write();
        if let Some(node) = inodes.get_mut(&inode) {
            let new_end = offset + n as u64;
            if new_end > node.size {
                node.size = new_end;
            }
            node.modified = get_current_time();
        }
        Ok(n)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let node = self.get_inode(inode)?;
        Ok(node.to_metadata())
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.permissions = metadata.permissions;
        node.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, dirname) = Self::split_path(path)?;
        if dirname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_ino = self.resolve_path(&parent_path)?;
        let new_ino = self.alloc_inode();
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_ino).ok_or(FsError::NotFound)?;
        if !parent.is_dir {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }
        let mut dir_inode = IomapInode::new_dir(new_ino, permissions);
        dir_inode.entries.insert(".".to_string(), new_ino);
        dir_inode.entries.insert("..".to_string(), parent_ino);
        parent.entries.insert(dirname, new_ino);
        parent.modified = get_current_time();
        parent.link_count = parent.link_count.saturating_add(1);
        inodes.insert(new_ino, dir_inode);
        Ok(new_ino)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        if path == "/" {
            return Err(FsError::PermissionDenied);
        }
        let target_ino = self.resolve_path(path)?;
        let (parent_path, dirname) = Self::split_path(path)?;
        let parent_ino = self.resolve_path(&parent_path)?;
        let mut inodes = self.inodes.write();
        let node = inodes.get(&target_ino).ok_or(FsError::NotFound)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }
        // Check empty (only . and ..).
        if node.entries.len() > 2 {
            return Err(FsError::DirectoryNotEmpty);
        }
        if let Some(parent) = inodes.get_mut(&parent_ino) {
            parent.entries.remove(&dirname);
            parent.modified = get_current_time();
            parent.link_count = parent.link_count.saturating_sub(1);
        }
        inodes.remove(&target_ino);
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let target_ino = self.resolve_path(path)?;
        let (parent_path, filename) = Self::split_path(path)?;
        let parent_ino = self.resolve_path(&parent_path)?;
        let mut inodes = self.inodes.write();
        let node = inodes.get(&target_ino).ok_or(FsError::NotFound)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        if let Some(parent) = inodes.get_mut(&parent_ino) {
            parent.entries.remove(&filename);
            parent.modified = get_current_time();
        }
        inodes.remove(&target_ino);
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }
        let mut entries = Vec::new();
        for (name, &child_ino) in &node.entries {
            let ft = inodes
                .get(&child_ino)
                .map(|n| n.file_type())
                .unwrap_or(FileType::Regular);
            entries.push(DirectoryEntry {
                name: name.clone(),
                inode: child_ino,
                file_type: ft,
            });
        }
        Ok(entries)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        if old_path == new_path {
            return Ok(());
        }
        let old_ino = self.resolve_path(old_path)?;
        let (old_parent_path, old_name) = Self::split_path(old_path)?;
        let (new_parent_path, new_name) = Self::split_path(new_path)?;
        let old_parent_ino = self.resolve_path(&old_parent_path)?;
        let new_parent_ino = self.resolve_path(&new_parent_path)?;
        if new_name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        // Check destination doesn't exist.
        let new_parent = inodes.get(&new_parent_ino).ok_or(FsError::NotFound)?;
        if new_parent.entries.contains_key(&new_name) {
            return Err(FsError::AlreadyExists);
        }
        // Remove from old parent.
        if let Some(op) = inodes.get_mut(&old_parent_ino) {
            op.entries.remove(&old_name);
            op.modified = get_current_time();
        }
        // Add to new parent.
        if let Some(np) = inodes.get_mut(&new_parent_ino) {
            np.entries.insert(new_name, old_ino);
            np.modified = get_current_time();
        }
        // Update node's ".." if it's a directory.
        if let Some(node) = inodes.get_mut(&old_ino) {
            if node.is_dir {
                node.entries.insert("..".to_string(), new_parent_ino);
                if old_parent_ino != new_parent_ino {
                    if let Some(op) = inodes.get_mut(&old_parent_ino) {
                        op.link_count = op.link_count.saturating_sub(1);
                    }
                    if let Some(np) = inodes.get_mut(&new_parent_ino) {
                        np.link_count = np.link_count.saturating_add(1);
                    }
                }
            }
        }
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_path, linkname) = Self::split_path(link_path)?;
        if linkname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_ino = self.resolve_path(&parent_path)?;
        let new_ino = self.alloc_inode();
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_ino).ok_or(FsError::NotFound)?;
        if !parent.is_dir {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&linkname) {
            return Err(FsError::AlreadyExists);
        }
        let link_inode = IomapInode::new_symlink(new_ino, target);
        parent.entries.insert(linkname, new_ino);
        parent.modified = get_current_time();
        inodes.insert(new_ino, link_inode);
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let ino = self.resolve_path(path)?;
        let node = self.get_inode(ino)?;
        if node.symlink_target.is_none() {
            return Err(FsError::InvalidArgument);
        }
        node.symlink_target.ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        // All writes go directly to the block device; the page cache is
        // write-through. Nothing to flush.
        Ok(())
    }
}
