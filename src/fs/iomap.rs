//! I/O mapping (iomap) infrastructure
//!
//! Provides an in-memory extent-map based I/O backend modelling Linux's
//! `fs/iomap` helper layer. Files are backed by a list of extents that may be
//! `Mapped` (backed by a real block), `Hole` (sparse, reads as zeroes),
//! `Unwritten` (allocated but never written) or `Delalloc` (delayed
//! allocation). Buffered I/O goes through a per-inode page cache; a direct-I/O
//! path bypasses the cache for block-aligned transfers.
//!
//! The type implements [`FileSystem`] so it can be mounted and exercised like a
//! real filesystem, while the extent/page-cache APIs mirror the iomap
//! interface used by ext4/xfs/etc.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use spin::RwLock;

/// Page size used by the iomap page cache (4 KiB).
const PAGE_SIZE: usize = 4096;
/// Block size for the backing block store (4 KiB).
const BLOCK_SIZE: u64 = 4096;

// ---------------------------------------------------------------------------
// Extent model
// ---------------------------------------------------------------------------

/// The kind of backing an extent has, mirroring `IOMAP_*` flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtentType {
    /// Backed by an allocated, written block (`IOMAP_MAPPED`).
    Mapped,
    /// A hole in the file (`IOMAP_HOLE`); reads return zeroes.
    Hole,
    /// Allocated but never written (`IOMAP_UNWRITTEN`).
    Unwritten,
    /// Delayed allocation (`IOMAP_DELALLOC`); no physical block yet.
    Delalloc,
}

/// A contiguous extent mapping a range of file bytes to backing storage.
#[derive(Debug, Clone)]
pub struct IomapExtent {
    /// Starting byte offset within the file.
    pub offset: u64,
    /// Length in bytes.
    pub length: u64,
    /// Kind of backing.
    pub kind: ExtentType,
    /// Physical block number when `Mapped`/`Unwritten`, else 0.
    pub block_start: u64,
}

impl IomapExtent {
    fn end(&self) -> u64 {
        self.offset + self.length
    }
}

/// Result of mapping a range of file bytes to extents.
#[derive(Debug, Clone)]
pub struct IomapMap {
    /// The extent covering (or overlapping) the requested offset.
    pub extent: IomapExtent,
    /// Number of bytes from `offset` covered by this extent.
    pub length: u64,
}

// ---------------------------------------------------------------------------
// Per-inode data
// ---------------------------------------------------------------------------

/// In-memory iomap inode.
#[derive(Debug, Clone)]
struct IomapNode {
    metadata: FileMetadata,
    /// Directory entries for directory inodes.
    entries: BTreeMap<String, InodeNumber>,
    /// Extent list for regular-file inodes, sorted by offset.
    extents: Vec<IomapExtent>,
    /// Page cache: page index -> page bytes.
    page_cache: BTreeMap<u64, Vec<u8>>,
    /// Dirty page indices awaiting writeback.
    dirty_pages: Vec<u64>,
    /// Symbolic link target.
    symlink_target: Option<String>,
}

impl IomapNode {
    fn new_file(inode: InodeNumber, permissions: FilePermissions) -> Self {
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::Regular,
                size: 0,
                permissions,
                uid: 0,
                gid: 0,
                created: get_current_time(),
                modified: get_current_time(),
                accessed: get_current_time(),
                link_count: 1,
                device_id: None,
            },
            entries: BTreeMap::new(),
            extents: Vec::new(),
            page_cache: BTreeMap::new(),
            dirty_pages: Vec::new(),
            symlink_target: None,
        }
    }

    fn new_directory(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), inode);
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::Directory,
                size: 0,
                permissions,
                uid: 0,
                gid: 0,
                created: get_current_time(),
                modified: get_current_time(),
                accessed: get_current_time(),
                link_count: 2,
                device_id: None,
            },
            entries,
            extents: Vec::new(),
            page_cache: BTreeMap::new(),
            dirty_pages: Vec::new(),
            symlink_target: None,
        }
    }

    fn new_symlink(inode: InodeNumber, target: &str) -> Self {
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::SymbolicLink,
                size: target.len() as u64,
                permissions: FilePermissions::from_octal(0o777),
                uid: 0,
                gid: 0,
                created: get_current_time(),
                modified: get_current_time(),
                accessed: get_current_time(),
                link_count: 1,
                device_id: None,
            },
            entries: BTreeMap::new(),
            extents: Vec::new(),
            page_cache: BTreeMap::new(),
            dirty_pages: Vec::new(),
            symlink_target: Some(target.to_string()),
        }
    }

    /// Find the extent covering `offset`, or the hole that contains it.
    fn map_blocks(&self, offset: u64) -> IomapMap {
        // Extents are sorted by offset; find the one containing `offset`.
        for ext in &self.extents {
            if offset >= ext.offset && offset < ext.end() {
                let length = ext.end() - offset;
                return IomapMap {
                    extent: ext.clone(),
                    length,
                };
            }
        }
        // No extent covers the offset: it's a hole. Compute the hole's length
        // up to the next extent (or EOF).
        let next_start = self
            .extents
            .iter()
            .map(|e| e.offset)
            .filter(|&start| start > offset)
            .min()
            .unwrap_or(self.metadata.size.max(offset + 1));
        let length = next_start - offset;
        IomapMap {
            extent: IomapExtent {
                offset,
                length,
                kind: ExtentType::Hole,
                block_start: 0,
            },
            length,
        }
    }

    /// Allocate (or convert) an extent covering `[offset, offset+length)` as
    /// `Mapped`, assigning physical block numbers from the block store.
    fn convert_extent(&mut self, offset: u64, length: u64, next_block: &mut u64) {
        // Round to block boundaries so the extent map stays block-aligned.
        let start_block = offset / BLOCK_SIZE;
        let end_block = (offset + length + BLOCK_SIZE - 1) / BLOCK_SIZE;
        let aligned_offset = start_block * BLOCK_SIZE;
        let aligned_length = (end_block - start_block) * BLOCK_SIZE;

        // Remove any existing extents that overlap the new range, preserving
        // the non-overlapping head/tail of partially overlapping extents.
        let mut kept: Vec<IomapExtent> = Vec::new();
        let new_end = aligned_offset + aligned_length;
        for ext in core::mem::take(&mut self.extents) {
            if ext.end() <= aligned_offset || ext.offset >= new_end {
                kept.push(ext);
                continue;
            }
            // Keep the leading non-overlapping portion.
            if ext.offset < aligned_offset {
                kept.push(IomapExtent {
                    offset: ext.offset,
                    length: aligned_offset - ext.offset,
                    kind: ext.kind,
                    block_start: ext.block_start,
                });
            }
            // Keep the trailing non-overlapping portion.
            if ext.end() > new_end {
                kept.push(IomapExtent {
                    offset: new_end,
                    length: ext.end() - new_end,
                    kind: ext.kind,
                    block_start: ext.block_start + (new_end - ext.offset) / BLOCK_SIZE,
                });
            }
        }
        let block_start = *next_block;
        *next_block += aligned_length / BLOCK_SIZE;
        kept.push(IomapExtent {
            offset: aligned_offset,
            length: aligned_length,
            kind: ExtentType::Mapped,
            block_start,
        });
        kept.sort_by_key(|e| e.offset);
        self.extents = kept;
    }

    /// Read a page from the page cache, populating it from the extent map on a
    /// miss. Hole/unwritten extents read as zeroes.
    fn read_page(&mut self, page_index: u64, block_store: &BlockStore) -> Vec<u8> {
        if let Some(page) = self.page_cache.get(&page_index) {
            return page.clone();
        }
        let offset = page_index * PAGE_SIZE as u64;
        let mut page = alloc::vec![0u8; PAGE_SIZE];
        // Walk extents overlapping this page and copy their data.
        let mut pos = 0u64;
        while pos < PAGE_SIZE as u64 {
            let file_off = offset + pos;
            if file_off >= self.metadata.size {
                break;
            }
            let map = self.map_blocks(file_off);
            let take = core::cmp::min(
                map.length,
                (self.metadata.size - file_off).min(PAGE_SIZE as u64 - pos),
            );
            match map.extent.kind {
                ExtentType::Mapped => {
                    let block_off =
                        (file_off - map.extent.offset) + map.extent.block_start * BLOCK_SIZE;
                    let bytes = block_store.read(block_off, take as usize);
                    let dst_start = pos as usize;
                    let copy_len = bytes.len();
                    page[dst_start..dst_start + copy_len].copy_from_slice(&bytes);
                }
                ExtentType::Hole | ExtentType::Unwritten | ExtentType::Delalloc => {
                    // Zeroes already fill the page.
                }
            }
            pos += take;
        }
        self.page_cache.insert(page_index, page.clone());
        page
    }

    /// Write a page into the page cache, marking it dirty. Extents are
    /// allocated lazily on writeback.
    fn write_page(&mut self, page_index: u64, data: &[u8]) {
        let mut page = self
            .page_cache
            .get(&page_index)
            .cloned()
            .unwrap_or_else(|| alloc::vec![0u8; PAGE_SIZE]);
        let len = core::cmp::min(data.len(), PAGE_SIZE);
        page[..len].copy_from_slice(&data[..len]);
        self.page_cache.insert(page_index, page);
        if !self.dirty_pages.contains(&page_index) {
            self.dirty_pages.push(page_index);
        }
    }

    /// Flush dirty pages back to the block store, converting delalloc extents
    /// to mapped extents as needed.
    fn writeback(&mut self, block_store: &mut BlockStore, next_block: &mut u64) -> FsResult<usize> {
        let dirty = core::mem::take(&mut self.dirty_pages);
        let mut flushed = 0;
        for page_index in dirty {
            let page = match self.page_cache.get(&page_index) {
                Some(p) => p.clone(),
                None => continue,
            };
            let offset = page_index * PAGE_SIZE as u64;
            // Ensure an extent covers this page.
            self.convert_extent(offset, PAGE_SIZE as u64, next_block);
            // Find the (now mapped) extent and write the page into the block store.
            let map = self.map_blocks(offset);
            if map.extent.kind == ExtentType::Mapped {
                let block_off = map.extent.block_start * BLOCK_SIZE;
                block_store.write(block_off, &page);
                flushed += 1;
            }
        }
        if flushed > 0 {
            self.metadata.modified = get_current_time();
        }
        Ok(flushed)
    }

    /// Direct I/O read: bypass the page cache and read straight from the block
    /// store. Requires block-aligned offset and buffer length.
    fn direct_read(
        &self,
        offset: u64,
        buffer: &mut [u8],
        block_store: &BlockStore,
    ) -> FsResult<usize> {
        if offset % BLOCK_SIZE != 0 || (buffer.len() as u64) % BLOCK_SIZE != 0 {
            return Err(FsError::InvalidArgument);
        }
        let mut read = 0usize;
        let mut pos = 0u64;
        while pos < buffer.len() as u64 {
            let file_off = offset + pos;
            if file_off >= self.metadata.size {
                break;
            }
            let map = self.map_blocks(file_off);
            let take = core::cmp::min(
                map.length,
                (self.metadata.size - file_off).min((buffer.len() as u64) - pos),
            );
            match map.extent.kind {
                ExtentType::Mapped => {
                    let block_off =
                        (file_off - map.extent.offset) + map.extent.block_start * BLOCK_SIZE;
                    let bytes = block_store.read(block_off, take as usize);
                    let dst = pos as usize;
                    buffer[dst..dst + bytes.len()].copy_from_slice(&bytes);
                }
                _ => {
                    // Holes read as zeroes.
                    let dst = pos as usize;
                    let end = dst + take as usize;
                    for b in &mut buffer[dst..end] {
                        *b = 0;
                    }
                }
            }
            pos += take;
            read += take as usize;
        }
        Ok(read)
    }

    /// Direct I/O write: bypass the page cache and write straight to the block
    /// store, allocating extents as needed. Requires block alignment.
    fn direct_write(
        &mut self,
        offset: u64,
        buffer: &[u8],
        block_store: &mut BlockStore,
        next_block: &mut u64,
    ) -> FsResult<usize> {
        if offset % BLOCK_SIZE != 0 || (buffer.len() as u64) % BLOCK_SIZE != 0 {
            return Err(FsError::InvalidArgument);
        }
        self.convert_extent(offset, buffer.len() as u64, next_block);
        let map = self.map_blocks(offset);
        if map.extent.kind != ExtentType::Mapped {
            return Err(FsError::IoError);
        }
        let block_off = map.extent.block_start * BLOCK_SIZE;
        block_store.write(block_off, buffer);
        let new_end = offset + buffer.len() as u64;
        if new_end > self.metadata.size {
            self.metadata.size = new_end;
        }
        self.metadata.modified = get_current_time();
        Ok(buffer.len())
    }
}

// ---------------------------------------------------------------------------
// Block store
// ---------------------------------------------------------------------------

/// Simple in-memory block store backing mapped extents.
#[derive(Debug, Default)]
struct BlockStore {
    blocks: BTreeMap<u64, Vec<u8>>,
}

impl BlockStore {
    fn read(&self, block_off: u64, len: usize) -> Vec<u8> {
        let first_block = block_off / BLOCK_SIZE;
        let mut out = Vec::with_capacity(len);
        let mut remaining = len;
        let mut block = first_block;
        let mut in_block_off = (block_off % BLOCK_SIZE) as usize;
        while remaining > 0 {
            let blk = self
                .blocks
                .get(&block)
                .cloned()
                .unwrap_or_else(|| alloc::vec![0u8; BLOCK_SIZE as usize]);
            let avail = (BLOCK_SIZE as usize) - in_block_off;
            let take = core::cmp::min(remaining, avail);
            out.extend_from_slice(&blk[in_block_off..in_block_off + take]);
            remaining -= take;
            block += 1;
            in_block_off = 0;
        }
        out
    }

    fn write(&mut self, block_off: u64, data: &[u8]) {
        let first_block = block_off / BLOCK_SIZE;
        let mut remaining = data.len();
        let mut block = first_block;
        let mut in_block_off = (block_off % BLOCK_SIZE) as usize;
        let mut src = 0usize;
        while remaining > 0 {
            let blk = self
                .blocks
                .entry(block)
                .or_insert_with(|| alloc::vec![0u8; BLOCK_SIZE as usize]);
            let avail = (BLOCK_SIZE as usize) - in_block_off;
            let take = core::cmp::min(remaining, avail);
            blk[in_block_off..in_block_off + take].copy_from_slice(&data[src..src + take]);
            remaining -= take;
            src += take;
            block += 1;
            in_block_off = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// Filesystem
// ---------------------------------------------------------------------------

/// Iomap-backed filesystem.
#[derive(Debug)]
pub struct IomapFileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, IomapNode>>,
    next_inode: RwLock<InodeNumber>,
    block_store: RwLock<BlockStore>,
    next_block: RwLock<u64>,
    root_inode: InodeNumber,
}

impl IomapFileSystem {
    /// Create a new iomap filesystem with an empty root directory.
    pub fn new() -> FsResult<Self> {
        let root_inode = 1;
        let mut inodes = BTreeMap::new();
        let mut root = IomapNode::new_directory(root_inode, FilePermissions::default_directory());
        root.entries.insert("..".to_string(), root_inode);
        inodes.insert(root_inode, root);
        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
            block_store: RwLock::new(BlockStore::default()),
            next_block: RwLock::new(0),
            root_inode,
        })
    }

    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;
        inode
    }

    fn split_path(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|c| !c.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(self.root_inode);
        }
        let components = Self::split_path(path);
        let inodes = self.inodes.read();
        let mut current = self.root_inode;
        for component in components {
            let node = inodes.get(&current).ok_or(FsError::NotFound)?;
            if node.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            current = *node.entries.get(&component).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }

    fn resolve_parent(&self, path: &str) -> FsResult<(InodeNumber, String)> {
        if path == "/" {
            return Err(FsError::InvalidArgument);
        }
        let components = Self::split_path(path);
        if components.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        let filename = components.last().unwrap().clone();
        if components.len() == 1 {
            return Ok((self.root_inode, filename));
        }
        let parent_path = format!("/{}", components[..components.len() - 1].join("/"));
        let parent_inode = self.resolve_path(&parent_path)?;
        Ok((parent_inode, filename))
    }

    /// Map the extents covering `[offset, offset+length)` for an inode.
    pub fn map_blocks(
        &self,
        inode: InodeNumber,
        offset: u64,
        length: u64,
    ) -> FsResult<Vec<IomapMap>> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        let mut maps = Vec::new();
        let mut pos = offset;
        let end = offset + length;
        while pos < end {
            let map = node.map_blocks(pos);
            if map.length == 0 {
                break;
            }
            let take = core::cmp::min(map.length, end - pos);
            maps.push(IomapMap {
                extent: map.extent.clone(),
                length: take,
            });
            pos += take;
        }
        Ok(maps)
    }

    /// Find the next data (non-hole) offset at or after `offset`.
    pub fn seek_data(&self, inode: InodeNumber, offset: u64) -> FsResult<u64> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        for ext in &node.extents {
            if ext.kind == ExtentType::Mapped && ext.end() > offset {
                return Ok(core::cmp::max(ext.offset, offset));
            }
        }
        Err(FsError::NotFound)
    }

    /// Find the next hole at or after `offset`.
    pub fn seek_hole(&self, inode: InodeNumber, offset: u64) -> FsResult<u64> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        let mut pos = offset;
        for ext in &node.extents {
            if ext.offset > pos {
                return Ok(pos);
            }
            pos = core::cmp::max(pos, ext.end());
        }
        if pos < node.metadata.size {
            Ok(pos)
        } else {
            Err(FsError::NotFound)
        }
    }

    /// Direct I/O read bypassing the page cache.
    pub fn direct_read(
        &self,
        inode: InodeNumber,
        offset: u64,
        buffer: &mut [u8],
    ) -> FsResult<usize> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        let store = self.block_store.read();
        node.direct_read(offset, buffer, &store)
    }

    /// Direct I/O write bypassing the page cache.
    pub fn direct_write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        let mut store = self.block_store.write();
        let mut next_block = self.next_block.write();
        node.direct_write(offset, buffer, &mut store, &mut next_block)
    }

    /// Flush dirty pages for an inode to the block store.
    pub fn writeback(&self, inode: InodeNumber) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        let mut store = self.block_store.write();
        let mut next_block = self.next_block.write();
        node.writeback(&mut store, &mut next_block)
    }
}

impl FileSystem for IomapFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Iomap
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let store = self.block_store.read();
        let used_blocks = store.blocks.len() as u64;
        let total_blocks = 65536u64;
        Ok(FileSystemStats {
            total_blocks,
            free_blocks: total_blocks.saturating_sub(used_blocks),
            available_blocks: total_blocks.saturating_sub(used_blocks),
            total_inodes: 4096,
            free_inodes: 4096u64.saturating_sub(inodes.len() as u64),
            block_size: BLOCK_SIZE as u32,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, filename) = self.resolve_parent(path)?;
        if filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let node = IomapNode::new_file(new_inode, permissions);
        parent.entries.insert(filename, new_inode);
        parent.metadata.modified = get_current_time();
        inodes.insert(new_inode, node);
        Ok(new_inode)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        if node.metadata.file_type == FileType::SymbolicLink {
            let target = node.symlink_target.clone().unwrap_or_default();
            let target_bytes = target.as_bytes();
            let len = target_bytes.len() as u64;
            if offset >= len {
                return Ok(0);
            }
            let start = offset as usize;
            let end = core::cmp::min(start + buffer.len(), target_bytes.len());
            let n = end - start;
            buffer[..n].copy_from_slice(&target_bytes[start..end]);
            return Ok(n);
        }
        node.metadata.accessed = get_current_time();
        let size = node.metadata.size;
        if offset >= size {
            return Ok(0);
        }
        // Buffered I/O: pull each overlapping page from the page cache.
        let store = self.block_store.read();
        let mut read = 0usize;
        let mut pos = 0u64;
        while pos < buffer.len() as u64 {
            let file_off = offset + pos;
            if file_off >= size {
                break;
            }
            let page_index = file_off / PAGE_SIZE as u64;
            let in_page = (file_off % PAGE_SIZE as u64) as usize;
            let avail = (PAGE_SIZE - in_page)
                .min((size - file_off) as usize)
                .min(buffer.len() - pos as usize);
            let page = node.read_page(page_index, &store);
            let dst = pos as usize;
            buffer[dst..dst + avail].copy_from_slice(&page[in_page..in_page + avail]);
            pos += avail as u64;
            read += avail;
        }
        Ok(read)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        // Buffered I/O: stage writes into the page cache; writeback happens on
        // sync() or explicit writeback().
        let mut pos = 0u64;
        while pos < buffer.len() as u64 {
            let file_off = offset + pos;
            let page_index = file_off / PAGE_SIZE as u64;
            let in_page = (file_off % PAGE_SIZE as u64) as usize;
            let avail = core::cmp::min(PAGE_SIZE - in_page, buffer.len() - pos as usize);
            node.write_page(page_index, &buffer[pos as usize..pos as usize + avail]);
            pos += avail as u64;
        }
        let new_end = offset + buffer.len() as u64;
        if new_end > node.metadata.size {
            node.metadata.size = new_end;
        }
        node.metadata.modified = get_current_time();
        // Eagerly write back so reads from the block store stay consistent.
        drop(inodes);
        let _ = self.writeback(inode);
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        Ok(node.metadata.clone())
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.metadata.permissions = metadata.permissions;
        node.metadata.uid = metadata.uid;
        node.metadata.gid = metadata.gid;
        node.metadata.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        if dirname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut dir = IomapNode::new_directory(new_inode, permissions);
        dir.entries.insert("..".to_string(), parent_inode);
        parent.entries.insert(dirname, new_inode);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count += 1;
        inodes.insert(new_inode, dir);
        Ok(new_inode)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        if path == "/" {
            return Err(FsError::PermissionDenied);
        }
        let target = self.resolve_path(path)?;
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let node = inodes.get(&target).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        // Only `.` and `..` may be present.
        if node.entries.len() > 2 {
            return Err(FsError::DirectoryNotEmpty);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&dirname);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count -= 1;
        inodes.remove(&target);
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let target = self.resolve_path(path)?;
        let (parent_inode, filename) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let node = inodes.get(&target).ok_or(FsError::NotFound)?;
        if node.metadata.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&filename);
        parent.metadata.modified = get_current_time();
        inodes.remove(&target);
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        node.metadata.accessed = get_current_time();
        let snapshot: Vec<(String, InodeNumber)> =
            node.entries.iter().map(|(n, &i)| (n.clone(), i)).collect();
        let mut entries = Vec::new();
        for (name, child_inode) in snapshot {
            if let Some(child) = inodes.get(&child_inode) {
                entries.push(DirectoryEntry {
                    name,
                    inode: child_inode,
                    file_type: child.metadata.file_type,
                });
            }
        }
        Ok(entries)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let target = self.resolve_path(old_path)?;
        let (old_parent, old_name) = self.resolve_parent(old_path)?;
        let (new_parent, new_name) = self.resolve_parent(new_path)?;
        if new_name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        let new_p = inodes.get(&new_parent).ok_or(FsError::NotFound)?;
        if new_p.entries.contains_key(&new_name) {
            return Err(FsError::AlreadyExists);
        }
        let old_p = inodes.get_mut(&old_parent).ok_or(FsError::NotFound)?;
        old_p.entries.remove(&old_name);
        let new_p = inodes.get_mut(&new_parent).ok_or(FsError::NotFound)?;
        new_p.entries.insert(new_name, target);
        new_p.metadata.modified = get_current_time();
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_inode, linkname) = self.resolve_parent(link_path)?;
        if linkname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&linkname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let node = IomapNode::new_symlink(new_inode, target);
        parent.entries.insert(linkname, new_inode);
        parent.metadata.modified = get_current_time();
        inodes.insert(new_inode, node);
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let target = self.resolve_path(path)?;
        let inodes = self.inodes.read();
        let node = inodes.get(&target).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        node.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        // Write back every dirty inode.
        let inode_ids: Vec<InodeNumber> = self.inodes.read().keys().copied().collect();
        for inode in inode_ids {
            let _ = self.writeback(inode);
        }
        Ok(())
    }
}
