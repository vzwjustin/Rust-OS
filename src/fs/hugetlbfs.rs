//! HugetlbFS in-memory filesystem for huge page memory pools
//!
//! Linux's hugetlbfs exposes huge pages (2 MiB, 1 GiB) as files backed by the
//! huge page pool. This in-memory implementation tracks the page pool state
//! (page size, free/used/max pages) and presents files whose sizes are
//! rounded up to the configured huge page size. Reads return zeroed memory
//! (the pool pages are anonymous); writes are accepted up to the allocated
//! page capacity.
//!
//! A real implementation would map huge pages from the hugetlb pool into the
//! file's page cache and fault them on access; the page accounting here is
//! the state the pool manager would track.

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
use core::cmp;
use spin::RwLock;

/// Default huge page size: 2 MiB.
const DEFAULT_HUGE_PAGE_SIZE: u64 = 2 * 1024 * 1024;
/// Default huge page pool capacity.
const DEFAULT_MAX_PAGES: u64 = 512;
/// Block size reported via statfs (matches the huge page size).
const BLOCK_SIZE: u32 = 4096;

/// In-memory hugetlbfs inode
#[derive(Debug, Clone)]
struct HugetlbInode {
    inode: InodeNumber,
    is_dir: bool,
    size: u64,
    permissions: FilePermissions,
    /// Number of huge pages allocated to this file.
    pages_allocated: u64,
    /// Backing bytes (one byte per page is enough to track allocation; real
    /// hugetlbfs would map actual huge pages). Length == pages_allocated.
    backing: Vec<u8>,
    /// Directory entries mapping name -> child inode number.
    entries: BTreeMap<String, InodeNumber>,
    /// Creation time (Unix timestamp).
    created: u64,
    /// Last modification time.
    modified: u64,
    /// Last access time.
    accessed: u64,
    /// Owner user ID.
    uid: u32,
    /// Owner group ID.
    gid: u32,
}

impl HugetlbInode {
    fn new_file(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        Self {
            inode,
            is_dir: false,
            size: 0,
            permissions,
            pages_allocated: 0,
            backing: Vec::new(),
            entries: BTreeMap::new(),
            created: now,
            modified: now,
            accessed: now,
            uid: 0,
            gid: 0,
        }
    }

    fn new_directory(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), inode);
        Self {
            inode,
            is_dir: true,
            size: 0,
            permissions,
            pages_allocated: 0,
            backing: Vec::new(),
            entries,
            created: now,
            modified: now,
            accessed: now,
            uid: 0,
            gid: 0,
        }
    }
}

/// In-memory hugetlbfs filesystem tracking the huge page pool.
#[derive(Debug)]
pub struct HugetlbFs {
    inodes: RwLock<BTreeMap<InodeNumber, HugetlbInode>>,
    next_inode: RwLock<InodeNumber>,
    /// Huge page size in bytes (e.g. 2 MiB).
    page_size: u64,
    /// Maximum number of huge pages the pool can hand out.
    max_pages: u64,
    /// Number of huge pages currently allocated to files.
    used_pages: RwLock<u64>,
}

impl HugetlbFs {
    /// Create a hugetlbfs with the default 2 MiB page size and 512-page pool.
    pub fn new() -> Self {
        Self::with_pool(DEFAULT_HUGE_PAGE_SIZE, DEFAULT_MAX_PAGES)
    }

    /// Create a hugetlbfs with a custom page size and pool capacity.
    pub fn with_pool(page_size: u64, max_pages: u64) -> Self {
        let mut inodes = BTreeMap::new();
        let mut root = HugetlbInode::new_directory(1, FilePermissions::default_directory());
        root.entries.insert("..".to_string(), 1);
        inodes.insert(1, root);
        Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
            page_size,
            max_pages,
            used_pages: RwLock::new(0),
        }
    }

    /// Huge page size in bytes.
    pub fn page_size(&self) -> u64 {
        self.page_size
    }

    /// Maximum number of huge pages in the pool.
    pub fn max_pages(&self) -> u64 {
        self.max_pages
    }

    /// Number of huge pages currently allocated.
    pub fn used_pages(&self) -> u64 {
        *self.used_pages.read()
    }

    /// Number of free huge pages in the pool.
    pub fn free_pages(&self) -> u64 {
        self.max_pages.saturating_sub(*self.used_pages.read())
    }

    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let n = *next;
        *next += 1;
        n
    }

    fn split_path(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|c| !c.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(1);
        }
        let components = Self::split_path(path);
        let inodes = self.inodes.read();
        let mut current = 1u64;
        for component in components {
            let node = inodes.get(&current).ok_or(FsError::NotFound)?;
            if !node.is_dir {
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
            Ok((1, filename))
        } else {
            let parent_path = format!("/{}", components[..components.len() - 1].join("/"));
            let parent_inode = self.resolve_path(&parent_path)?;
            Ok((parent_inode, filename))
        }
    }

    fn get_node(&self, inode: InodeNumber) -> FsResult<HugetlbInode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }

    fn is_directory_empty(inode: &HugetlbInode) -> bool {
        inode.entries.len() <= 2
    }

    /// Pages required to cover `size` bytes, rounded up to the huge page size.
    fn pages_for_size(&self, size: u64) -> u64 {
        if size == 0 {
            0
        } else {
            (size + self.page_size - 1) / self.page_size
        }
    }
}

impl FileSystem for HugetlbFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::HugetlbFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let used_inodes = inodes.len() as u64;
        let used_pages = *self.used_pages.read();
        let total_blocks = self.max_pages * (self.page_size / BLOCK_SIZE as u64);
        let free_blocks = self
            .free_pages()
            .saturating_mul(self.page_size / BLOCK_SIZE as u64);
        Ok(FileSystemStats {
            total_blocks,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: self.max_pages,
            free_inodes: self.max_pages.saturating_sub(used_inodes),
            block_size: BLOCK_SIZE,
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
        if !parent.is_dir {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let node = HugetlbInode::new_file(new_inode, permissions);
        parent.entries.insert(filename, new_inode);
        parent.modified = get_current_time();
        inodes.insert(new_inode, node);
        Ok(new_inode)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        node.accessed = get_current_time();
        // Reads beyond the file's logical size return 0.
        if offset >= node.size {
            return Ok(0);
        }
        let start = offset as usize;
        let end = cmp::min(start + buffer.len(), node.size as usize);
        let n = end - start;
        // Huge pages are anonymous zeroed memory; fill the buffer with zeros
        // up to the file's logical size. A real implementation would copy
        // from the mapped huge page.
        for b in buffer[..n].iter_mut() {
            *b = 0;
        }
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        let new_size = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        let needed_pages = self.pages_for_size(new_size);
        let current_pages = node.pages_allocated;
        let extra = needed_pages.saturating_sub(current_pages);

        // Reserve extra huge pages from the pool.
        let mut used = self.used_pages.write();
        if *used + extra > self.max_pages {
            return Err(FsError::NoSpaceLeft);
        }
        *used += extra;
        node.pages_allocated = needed_pages;
        node.backing.resize(needed_pages as usize, 0);
        node.size = new_size;
        node.modified = get_current_time();
        node.accessed = get_current_time();
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let node = self.get_node(inode)?;
        Ok(FileMetadata {
            inode,
            file_type: if node.is_dir {
                FileType::Directory
            } else {
                FileType::Regular
            },
            size: node.size,
            permissions: node.permissions,
            uid: node.uid,
            gid: node.gid,
            created: node.created,
            modified: node.modified,
            accessed: node.accessed,
            link_count: 1,
            device_id: None,
        })
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.permissions = metadata.permissions;
        node.uid = metadata.uid;
        node.gid = metadata.gid;
        if !node.is_dir {
            // Truncating/extending adjusts the page reservation.
            let target_pages = self.pages_for_size(metadata.size);
            let mut used = self.used_pages.write();
            match target_pages.cmp(&node.pages_allocated) {
                core::cmp::Ordering::Less => {
                    let freed = node.pages_allocated - target_pages;
                    *used = used.saturating_sub(freed);
                    node.pages_allocated = target_pages;
                    node.backing.truncate(target_pages as usize);
                    node.size = metadata.size;
                }
                core::cmp::Ordering::Greater => {
                    let extra = target_pages - node.pages_allocated;
                    if *used + extra > self.max_pages {
                        return Err(FsError::NoSpaceLeft);
                    }
                    *used += extra;
                    node.pages_allocated = target_pages;
                    node.backing.resize(target_pages as usize, 0);
                    node.size = metadata.size;
                }
                core::cmp::Ordering::Equal => {
                    node.size = metadata.size;
                }
            }
        }
        node.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        if dirname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if !parent.is_dir {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut dir = HugetlbInode::new_directory(new_inode, permissions);
        dir.entries.insert("..".to_string(), parent_inode);
        parent.entries.insert(dirname, new_inode);
        parent.modified = get_current_time();
        inodes.insert(new_inode, dir);
        Ok(new_inode)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        if path == "/" {
            return Err(FsError::PermissionDenied);
        }
        let dir_inode = self.resolve_path(path)?;
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let node = inodes.get(&dir_inode).ok_or(FsError::NotFound)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }
        if !Self::is_directory_empty(node) {
            return Err(FsError::DirectoryNotEmpty);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&dirname);
        parent.modified = get_current_time();
        inodes.remove(&dir_inode);
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let file_inode = self.resolve_path(path)?;
        let (parent_inode, filename) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let node = inodes.get(&file_inode).ok_or(FsError::NotFound)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        // Return the file's huge pages to the pool.
        let freed = node.pages_allocated;
        if freed > 0 {
            let mut used = self.used_pages.write();
            *used = used.saturating_sub(freed);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&filename);
        parent.modified = get_current_time();
        inodes.remove(&file_inode);
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut inodes = self.inodes.write();
        let dir = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if !dir.is_dir {
            return Err(FsError::NotADirectory);
        }
        dir.accessed = get_current_time();
        let snapshot: Vec<(String, InodeNumber)> =
            dir.entries.iter().map(|(n, &i)| (n.clone(), i)).collect();
        let mut out = Vec::with_capacity(snapshot.len());
        for (name, child_inode) in snapshot {
            if let Some(child) = inodes.get(&child_inode) {
                out.push(DirectoryEntry {
                    name,
                    inode: child_inode,
                    file_type: if child.is_dir {
                        FileType::Directory
                    } else {
                        FileType::Regular
                    },
                });
            }
        }
        Ok(out)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let old_inode = self.resolve_path(old_path)?;
        let (old_parent, old_name) = self.resolve_parent(old_path)?;
        let (new_parent, new_name) = self.resolve_parent(new_path)?;
        if new_name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        let new_parent_node = inodes.get(&new_parent).ok_or(FsError::NotFound)?;
        if !new_parent_node.is_dir {
            return Err(FsError::NotADirectory);
        }
        if new_parent_node.entries.contains_key(&new_name) {
            return Err(FsError::AlreadyExists);
        }
        let old_parent_node = inodes.get_mut(&old_parent).ok_or(FsError::NotFound)?;
        if !old_parent_node.is_dir {
            return Err(FsError::NotADirectory);
        }
        old_parent_node.entries.remove(&old_name);
        old_parent_node.modified = get_current_time();
        let new_parent_node = inodes.get_mut(&new_parent).ok_or(FsError::NotFound)?;
        new_parent_node.entries.insert(new_name, old_inode);
        new_parent_node.modified = get_current_time();
        Ok(())
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        // hugetlbfs does not support symlinks.
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        // Huge pages are anonymous; there is no backing store to sync.
        Ok(())
    }
}
