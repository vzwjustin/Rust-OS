//! HugetlbFS filesystem for huge page memory pools
//!
//! A virtual filesystem that provides access to huge memory pages
//! (2 MiB, 1 GiB, etc.) for performance-critical applications.
//! Files are backed by page-size-aligned allocations via the global
//! allocator, with allocation-on-first-write and zero-fill for unwritten
//! pages.  Truncation rounds the size up to a page multiple.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    alloc::{alloc, dealloc, Layout},
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;
use spin::RwLock;

/// Default total pool size in pages (256 pages → 512 MiB at 2 MiB).
const DEFAULT_POOL_PAGES: u64 = 256;

/// A single huge page: page-size-aligned allocation that is freed on Drop.
#[derive(Debug)]
pub struct HugePage {
    ptr: *mut u8,
    layout: Layout,
}

unsafe impl Send for HugePage {}
unsafe impl Sync for HugePage {}

impl HugePage {
    /// Allocate a zeroed huge page of `page_size` bytes.
    pub fn new(page_size: usize) -> Option<Self> {
        let layout = Layout::from_size_align(page_size, page_size).ok()?;
        // SAFETY: `layout` is valid (non-zero size, power-of-two align) and
        // we immediately zero-fill the returned allocation.
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            return None;
        }
        // SAFETY: `ptr` is valid for `page_size` bytes, just allocated.
        unsafe { core::ptr::write_bytes(ptr, 0, page_size) };
        Some(Self { ptr, layout })
    }

    /// Read `len` bytes from `offset` within this page into `buf`.
    pub fn read(&self, offset: usize, buf: &mut [u8]) {
        let end = core::cmp::min(offset + buf.len(), self.layout.size());
        if offset >= end {
            return;
        }
        let n = end - offset;
        // SAFETY: `ptr` is valid for `layout.size()` bytes; offset+n ≤ size.
        unsafe {
            core::ptr::copy_nonoverlapping(
                self.ptr.add(offset),
                buf.as_mut_ptr(),
                n,
            );
        }
    }

    /// Write `data` starting at `offset` within this page.
    pub fn write(&self, offset: usize, data: &[u8]) {
        let end = offset + data.len();
        if end > self.layout.size() {
            return;
        }
        // SAFETY: `ptr` is valid for `layout.size()` bytes; end ≤ size.
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                self.ptr.add(offset),
                data.len(),
            );
        }
    }
}

impl Drop for HugePage {
    fn drop(&mut self) {
        // SAFETY: `ptr` was allocated with `layout` via `alloc` and is unique.
        unsafe { dealloc(self.ptr, self.layout) };
    }
}

/// Huge-page inode metadata.
#[derive(Debug)]
struct HugeInode {
    metadata: FileMetadata,
    entries: BTreeMap<String, InodeNumber>,
    symlink_target: Option<String>,
}

/// HugetlbFS filesystem instance.
#[derive(Debug)]
pub struct HugetlbFs {
    page_size: usize,
    total_pages: u64,
    inodes: RwLock<BTreeMap<InodeNumber, HugeInode>>,
    pages: RwLock<BTreeMap<InodeNumber, Vec<HugePage>>>,
    next_inode: RwLock<InodeNumber>,
    root_inode: InodeNumber,
}

impl fmt::Display for HugetlbFs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "hugetlbfs(page_size={})", self.page_size)
    }
}

impl HugetlbFs {
    /// Create a new HugetlbFS with the given page size.
    pub fn new(page_size: usize) -> Self {
        let root_inode = 1;
        let mut inodes = BTreeMap::new();
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), root_inode);
        entries.insert("..".to_string(), root_inode);
        inodes.insert(
            root_inode,
            HugeInode {
                metadata: FileMetadata {
                    inode: root_inode,
                    file_type: FileType::Directory,
                    size: 0,
                    permissions: FilePermissions::from_octal(0o1777),
                    uid: 0,
                    gid: 0,
                    created: get_current_time(),
                    modified: get_current_time(),
                    accessed: get_current_time(),
                    link_count: 2,
                    device_id: None,
                },
                entries,
                symlink_target: None,
            },
        );

        Self {
            page_size,
            total_pages: DEFAULT_POOL_PAGES,
            inodes: RwLock::new(inodes),
            pages: RwLock::new(BTreeMap::new()),
            next_inode: RwLock::new(2),
            root_inode,
        }
    }

    fn alloc_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;
        inode
    }

    fn split_path(path: &str) -> FsResult<(String, String)> {
        if path.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        let trimmed = path.trim_start_matches('/');
        if trimmed.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        let last_slash = trimmed.rfind('/');
        match last_slash {
            Some(idx) => {
                let parent = format!("/{}", &trimmed[..idx]);
                let name = trimmed[idx + 1..].to_string();
                Ok((parent, name))
            }
            None => Ok(("/".to_string(), trimmed.to_string())),
        }
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" || path.is_empty() {
            return Ok(self.root_inode);
        }
        let inodes = self.inodes.read();
        let mut current = self.root_inode;
        for component in path.split('/').filter(|s| !s.is_empty()) {
            let node = inodes.get(&current).ok_or(FsError::NotFound)?;
            if node.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            current = *node.entries.get(component).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }

    /// Round a size up to a page multiple.
    fn round_to_page(&self, size: u64) -> u64 {
        let ps = self.page_size as u64;
        (size + ps - 1) / ps * ps
    }

    /// Ensure `count` pages are allocated for `inode`.
    fn ensure_pages(&self, inode: InodeNumber, count: usize) -> FsResult<()> {
        let mut pages = self.pages.write();
        let entry = pages.entry(inode).or_insert_with(Vec::new);
        while entry.len() < count {
            match HugePage::new(self.page_size) {
                Some(p) => entry.push(p),
                None => return Err(FsError::NoSpaceLeft),
            }
        }
        Ok(())
    }

    /// Count total pages currently allocated across all inodes.
    fn used_pages(&self) -> u64 {
        self.pages.read().values().map(|v| v.len() as u64).sum()
    }

    fn is_dir_empty(&self, inode: InodeNumber) -> FsResult<bool> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        // Only "." and ".." entries
        Ok(node.entries.len() <= 2)
    }
}

impl FileSystem for HugetlbFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::HugetlbFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let used = self.used_pages();
        let total = self.total_pages;
        let free = total.saturating_sub(used);
        Ok(FileSystemStats {
            total_blocks: total,
            free_blocks: free,
            available_blocks: free,
            total_inodes: self.total_pages * 4,
            free_inodes: self.total_pages * 4 - self.inodes.read().len() as u64,
            block_size: self.page_size as u32,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = Self::split_path(path)?;
        if name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_inode = self.resolve_path(&parent_path)?;
        let new_inode = self.alloc_inode();
        let mut inodes = self.inodes.write();
        {
            let parent = inodes.get(&parent_inode).ok_or(FsError::NotFound)?;
            if parent.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            if parent.entries.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        }
        inodes.insert(
            new_inode,
            HugeInode {
                metadata: FileMetadata {
                    inode: new_inode,
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
                symlink_target: None,
            },
        );
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.insert(name, new_inode);
        parent.metadata.modified = get_current_time();
        Ok(new_inode)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        // Resolve path; if not found and flags.create, synthesize
        match self.resolve_path(path) {
            Ok(inode) => Ok(inode),
            Err(FsError::NotFound) => {
                // Synthesize a new file
                self.create(path, FilePermissions::default_file())
            }
            Err(e) => Err(e),
        }
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        let file_size = node.metadata.size;
        drop(inodes);

        if offset >= file_size {
            return Ok(0);
        }
        let bytes_to_read = core::cmp::min(buffer.len() as u64, file_size - offset) as usize;
        let ps = self.page_size as u64;
        let pages = self.pages.read();

        let mut read = 0usize;
        let mut cur = offset;
        while read < bytes_to_read {
            let page_index = (cur / ps) as usize;
            let offset_in_page = (cur % ps) as usize;
            let remaining = bytes_to_read - read;
            let chunk = core::cmp::min(remaining, self.page_size - offset_in_page);

            if let Some(page_list) = pages.get(&inode) {
                if let Some(page) = page_list.get(page_index) {
                    page.read(offset_in_page, &mut buffer[read..read + chunk]);
                } else {
                    // Unwritten page — zero-fill
                    buffer[read..read + chunk].fill(0);
                }
            } else {
                buffer[read..read + chunk].fill(0);
            }

            read += chunk;
            cur += chunk as u64;
        }

        // Update access time
        drop(pages);
        let mut inodes = self.inodes.write();
        if let Some(node) = inodes.get_mut(&inode) {
            node.metadata.accessed = get_current_time();
        }

        Ok(read)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        // Validate it's a regular file
        {
            let inodes = self.inodes.read();
            let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
            if node.metadata.file_type != FileType::Regular {
                return Err(FsError::IsADirectory);
            }
        }

        let new_size = offset.checked_add(buffer.len() as u64).ok_or(FsError::InvalidArgument)?;
        let aligned_size = self.round_to_page(new_size);
        let pages_needed = (aligned_size / self.page_size as u64) as usize;

        // Check pool capacity
        let current_pages = self.pages.read().get(&inode).map(|v| v.len()).unwrap_or(0);
        let extra = pages_needed.saturating_sub(current_pages);
        let total_used = self.used_pages();
        if total_used + extra as u64 > self.total_pages {
            return Err(FsError::NoSpaceLeft);
        }

        // Allocate pages on first write
        self.ensure_pages(inode, pages_needed)?;

        // Write data page by page
        let ps = self.page_size as u64;
        let pages = self.pages.read();
        if let Some(page_list) = pages.get(&inode) {
            let mut written = 0usize;
            let mut cur = offset;
            while written < buffer.len() {
                let page_index = (cur / ps) as usize;
                let offset_in_page = (cur % ps) as usize;
                let chunk = core::cmp::min(buffer.len() - written, self.page_size - offset_in_page);
                if let Some(page) = page_list.get(page_index) {
                    page.write(offset_in_page, &buffer[written..written + chunk]);
                }
                written += chunk;
                cur += chunk as u64;
            }
        }
        drop(pages);

        // Update metadata
        let mut inodes = self.inodes.write();
        if let Some(node) = inodes.get_mut(&inode) {
            if new_size > node.metadata.size {
                node.metadata.size = new_size;
            }
            node.metadata.modified = get_current_time();
            node.metadata.accessed = get_current_time();
        }

        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        Ok(node.metadata.clone())
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        // Truncation must be a page multiple
        let aligned = self.round_to_page(metadata.size);
        let pages_needed = (aligned / self.page_size as u64) as usize;

        let current_pages = self.pages.read().get(&inode).map(|v| v.len()).unwrap_or(0);
        if pages_needed > current_pages {
            let extra = pages_needed - current_pages;
            if self.used_pages() + extra as u64 > self.total_pages {
                return Err(FsError::NoSpaceLeft);
            }
            self.ensure_pages(inode, pages_needed)?;
        } else if pages_needed < current_pages {
            let mut pages = self.pages.write();
            if let Some(page_list) = pages.get_mut(&inode) {
                page_list.truncate(pages_needed);
            }
        }

        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.metadata.size = aligned;
        node.metadata.permissions = metadata.permissions;
        node.metadata.uid = metadata.uid;
        node.metadata.gid = metadata.gid;
        node.metadata.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = Self::split_path(path)?;
        if name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_inode = self.resolve_path(&parent_path)?;
        let new_inode = self.alloc_inode();
        let mut inodes = self.inodes.write();
        {
            let parent = inodes.get(&parent_inode).ok_or(FsError::NotFound)?;
            if parent.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            if parent.entries.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        }
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), new_inode);
        entries.insert("..".to_string(), parent_inode);
        inodes.insert(
            new_inode,
            HugeInode {
                metadata: FileMetadata {
                    inode: new_inode,
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
                symlink_target: None,
            },
        );
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.insert(name, new_inode);
        parent.metadata.link_count += 1;
        parent.metadata.modified = get_current_time();
        Ok(new_inode)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        if path == "/" {
            return Err(FsError::PermissionDenied);
        }
        let dir_inode = self.resolve_path(path)?;
        if !self.is_dir_empty(dir_inode)? {
            return Err(FsError::DirectoryNotEmpty);
        }
        let (parent_path, name) = Self::split_path(path)?;
        let parent_inode = self.resolve_path(&parent_path)?;
        let mut inodes = self.inodes.write();
        inodes.remove(&dir_inode);
        if let Some(parent) = inodes.get_mut(&parent_inode) {
            parent.entries.remove(&name);
            parent.metadata.link_count -= 1;
            parent.metadata.modified = get_current_time();
        }
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let file_inode = self.resolve_path(path)?;
        let (parent_path, name) = Self::split_path(path)?;
        let parent_inode = self.resolve_path(&parent_path)?;
        let mut inodes = self.inodes.write();
        {
            let node = inodes.get(&file_inode).ok_or(FsError::NotFound)?;
            if node.metadata.file_type == FileType::Directory {
                return Err(FsError::IsADirectory);
            }
        }
        // Free pages
        self.pages.write().remove(&file_inode);
        inodes.remove(&file_inode);
        if let Some(parent) = inodes.get_mut(&parent_inode) {
            parent.entries.remove(&name);
            parent.metadata.modified = get_current_time();
        }
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        let mut entries = Vec::new();
        for (name, &child_inode) in &node.entries {
            let child = inodes.get(&child_inode).ok_or(FsError::NotFound)?;
            entries.push(DirectoryEntry {
                name: name.clone(),
                inode: child_inode,
                file_type: child.metadata.file_type,
            });
        }
        Ok(entries)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let old_inode = self.resolve_path(old_path)?;
        let (old_parent_path, old_name) = Self::split_path(old_path)?;
        let (new_parent_path, new_name) = Self::split_path(new_path)?;
        if new_name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let old_parent = self.resolve_path(&old_parent_path)?;
        let new_parent = self.resolve_path(&new_parent_path)?;
        let mut inodes = self.inodes.write();
        {
            let np = inodes.get(&new_parent).ok_or(FsError::NotFound)?;
            if np.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            if np.entries.contains_key(&new_name) {
                return Err(FsError::AlreadyExists);
            }
        }
        if let Some(op) = inodes.get_mut(&old_parent) {
            op.entries.remove(&old_name);
            op.metadata.modified = get_current_time();
        }
        if let Some(np) = inodes.get_mut(&new_parent) {
            np.entries.insert(new_name, old_inode);
            np.metadata.modified = get_current_time();
        }
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_path, name) = Self::split_path(link_path)?;
        if name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_inode = self.resolve_path(&parent_path)?;
        let new_inode = self.alloc_inode();
        let mut inodes = self.inodes.write();
        {
            let parent = inodes.get(&parent_inode).ok_or(FsError::NotFound)?;
            if parent.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            if parent.entries.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        }
        inodes.insert(
            new_inode,
            HugeInode {
                metadata: FileMetadata {
                    inode: new_inode,
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
                symlink_target: Some(target.to_string()),
            },
        );
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.insert(name, new_inode);
        parent.metadata.modified = get_current_time();
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let inode = self.resolve_path(path)?;
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        node.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
