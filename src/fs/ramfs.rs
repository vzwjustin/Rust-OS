//! RAM-based filesystem implementation
//!
//! This module provides a simple in-memory filesystem that stores
//! all files and directories in RAM. Useful for temporary storage
//! and as the root filesystem during boot.

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

/// Maximum file size in RAM filesystem (16MB)
const MAX_FILE_SIZE: u64 = 16 * 1024 * 1024;

/// Maximum number of files
const MAX_FILES: u64 = 4096;

/// RAM filesystem inode
#[derive(Debug, Clone)]
struct RamInode {
    /// Inode metadata
    metadata: FileMetadata,
    /// File content (for regular files)
    content: Vec<u8>,
    /// Directory entries (for directories)
    entries: BTreeMap<String, InodeNumber>,
    /// Symbolic link target (for symlinks)
    symlink_target: Option<String>,
}

impl RamInode {
    /// Create a new file inode
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
            content: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: None,
        }
    }

    /// Create a new directory inode
    fn new_directory(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), inode);
        // Parent will be set by the caller

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
                link_count: 2, // . and parent reference
                device_id: None,
            },
            content: Vec::new(),
            entries,
            symlink_target: None,
        }
    }

    /// Create a new symbolic link inode
    fn new_symlink(inode: InodeNumber, target: &str, permissions: FilePermissions) -> Self {
        Self {
            metadata: FileMetadata {
                inode,
                file_type: FileType::SymbolicLink,
                size: target.len() as u64,
                permissions,
                uid: 0,
                gid: 0,
                created: get_current_time(),
                modified: get_current_time(),
                accessed: get_current_time(),
                link_count: 1,
                device_id: None,
            },
            content: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: Some(target.to_string()),
        }
    }
}

/// RAM-based filesystem
#[derive(Debug)]
pub struct RamFs {
    /// All inodes in the filesystem
    inodes: RwLock<BTreeMap<InodeNumber, RamInode>>,
    /// Next inode number to allocate
    next_inode: RwLock<InodeNumber>,
    /// Root directory inode
    root_inode: InodeNumber,
}

impl RamFs {
    /// Create a new RAM filesystem
    pub fn new() -> Self {
        let root_inode = 1;
        let mut inodes = BTreeMap::new();

        // Create root directory
        let mut root = RamInode::new_directory(root_inode, FilePermissions::default_directory());
        root.entries.insert("..".to_string(), root_inode); // Root parent is itself
        inodes.insert(root_inode, root);

        Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
            root_inode,
        }
    }

    /// Allocate a new inode number
    fn allocate_inode(&self) -> InodeNumber {
        let mut next_inode = self.next_inode.write();
        let inode = *next_inode;
        *next_inode += 1;
        inode
    }

    /// Split path into components
    fn split_path(&self, path: &str) -> Vec<String> {
        path.split('/')
            .filter(|c| !c.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    /// Resolve path to inode number
    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(self.root_inode);
        }

        let components = self.split_path(path);
        let inodes = self.inodes.read();
        let mut current_inode = self.root_inode;

        for component in components {
            let inode = inodes.get(&current_inode).ok_or(FsError::NotFound)?;

            if inode.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }

            current_inode = *inode.entries.get(&component).ok_or(FsError::NotFound)?;
        }

        Ok(current_inode)
    }

    /// Get parent directory inode and filename from path
    fn resolve_parent(&self, path: &str) -> FsResult<(InodeNumber, String)> {
        if path == "/" {
            return Err(FsError::InvalidArgument);
        }

        let components = self.split_path(path);
        if components.is_empty() {
            return Err(FsError::InvalidArgument);
        }

        let filename = components.last().unwrap().clone();

        if components.len() == 1 {
            // File in root directory
            Ok((self.root_inode, filename))
        } else {
            // Resolve parent directory
            let parent_path = format!("/{}", components[..components.len() - 1].join("/"));
            let parent_inode = self.resolve_path(&parent_path)?;
            Ok((parent_inode, filename))
        }
    }

    /// Check if directory is empty (except for . and ..)
    fn is_directory_empty(&self, inode: InodeNumber) -> FsResult<bool> {
        let inodes = self.inodes.read();
        let dir_inode = inodes.get(&inode).ok_or(FsError::NotFound)?;

        if dir_inode.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        // Directory is empty if it only contains . and ..
        Ok(dir_inode.entries.len() <= 2)
    }
}

impl FileSystem for RamFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::RamFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let used_inodes = inodes.len() as u64;

        // Calculate total size used
        let _total_size: u64 = inodes
            .values()
            .map(|inode| inode.content.len() as u64)
            .sum();

        // Calculate real block-based statistics from actual filesystem state
        let block_size = 4096u32;
        let total_blocks = (MAX_FILE_SIZE * MAX_FILES) / block_size as u64;

        // Calculate actual used blocks by summing up all file sizes
        let used_blocks = inodes
            .values()
            .map(|inode| {
                // Round up file size to nearest block boundary
                let file_size = inode.content.len() as u64;
                (file_size + block_size as u64 - 1) / block_size as u64
            })
            .sum();

        let free_blocks = total_blocks.saturating_sub(used_blocks);

        Ok(FileSystemStats {
            total_blocks,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: MAX_FILES,
            free_inodes: MAX_FILES.saturating_sub(used_inodes),
            block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, filename) = self.resolve_parent(path)?;

        if filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }

        let mut inodes = self.inodes.write();

        // Check inode limit first
        if inodes.len() >= MAX_FILES as usize {
            return Err(FsError::NoSpaceLeft);
        }

        // Check if parent exists and is a directory
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        // Check if file already exists
        if parent.entries.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }

        // Create new file inode
        let new_inode = self.allocate_inode();
        let file_inode = RamInode::new_file(new_inode, permissions);

        // Add to parent directory
        parent.entries.insert(filename, new_inode);
        parent.metadata.modified = get_current_time();

        // Insert new inode
        inodes.insert(new_inode, file_inode);

        Ok(new_inode)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let file_inode = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;

        if file_inode.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }

        // Update access time
        file_inode.metadata.accessed = get_current_time();

        let content_len = file_inode.content.len() as u64;
        if offset >= content_len {
            return Ok(0);
        }

        let start = offset as usize;
        let end = cmp::min(start + buffer.len(), file_inode.content.len());
        let bytes_to_read = end - start;

        buffer[..bytes_to_read].copy_from_slice(&file_inode.content[start..end]);
        Ok(bytes_to_read)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let file_inode = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;

        if file_inode.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }

        let new_size = offset + buffer.len() as u64;
        if new_size > MAX_FILE_SIZE {
            return Err(FsError::NoSpaceLeft);
        }

        // Extend content if necessary
        let required_len = (offset + buffer.len() as u64) as usize;
        if file_inode.content.len() < required_len {
            file_inode.content.resize(required_len, 0);
        }

        // Write data
        let start = offset as usize;
        let end = start + buffer.len();
        file_inode.content[start..end].copy_from_slice(buffer);

        // Update metadata
        file_inode.metadata.size = file_inode.content.len() as u64;
        file_inode.metadata.modified = get_current_time();
        file_inode.metadata.accessed = get_current_time();

        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let inodes = self.inodes.read();
        let file_inode = inodes.get(&inode).ok_or(FsError::NotFound)?;
        Ok(file_inode.metadata.clone())
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let file_inode = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;

        // Update modifiable fields
        file_inode.metadata.permissions = metadata.permissions;
        file_inode.metadata.uid = metadata.uid;
        file_inode.metadata.gid = metadata.gid;
        file_inode.metadata.modified = get_current_time();

        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, dirname) = self.resolve_parent(path)?;

        if dirname.len() > 255 {
            return Err(FsError::NameTooLong);
        }

        let mut inodes = self.inodes.write();

        // Check inode limit first
        if inodes.len() >= MAX_FILES as usize {
            return Err(FsError::NoSpaceLeft);
        }

        // Check if parent exists and is a directory
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        // Check if directory already exists
        if parent.entries.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }

        // Create new directory inode
        let new_inode = self.allocate_inode();
        let mut dir_inode = RamInode::new_directory(new_inode, permissions);
        dir_inode.entries.insert("..".to_string(), parent_inode);

        // Add to parent directory
        parent.entries.insert(dirname, new_inode);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count += 1; // New subdirectory adds a link

        // Insert new inode
        inodes.insert(new_inode, dir_inode);

        Ok(new_inode)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        if path == "/" {
            return Err(FsError::PermissionDenied);
        }

        let dir_inode = self.resolve_path(path)?;

        // Check if directory is empty
        if !self.is_directory_empty(dir_inode)? {
            return Err(FsError::DirectoryNotEmpty);
        }

        let (parent_inode, dirname) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();

        // Remove from parent directory
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&dirname);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count -= 1;

        // Remove the directory inode
        inodes.remove(&dir_inode);

        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let file_inode = self.resolve_path(path)?;
        let (parent_inode, filename) = self.resolve_parent(path)?;

        let mut inodes = self.inodes.write();

        // Check if it's a directory
        let file = inodes.get(&file_inode).ok_or(FsError::NotFound)?;
        if file.metadata.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }

        // Remove from parent directory
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&filename);
        parent.metadata.modified = get_current_time();

        // Remove the file inode
        inodes.remove(&file_inode);

        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut inodes = self.inodes.write();
        let dir_inode = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;

        if dir_inode.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        // Update access time and snapshot the (name, inode) pairs. Collecting
        // here ends the mutable borrow of dir_inode so the directory-entry
        // lookups below can re-borrow `inodes` immutably — re-locking the same
        // non-reentrant RwLock (a second self.inodes.read()) would deadlock.
        dir_inode.metadata.accessed = get_current_time();
        let entry_list: Vec<(alloc::string::String, InodeNumber)> = dir_inode
            .entries
            .iter()
            .map(|(name, &entry_inode)| (name.clone(), entry_inode))
            .collect();

        let mut entries = Vec::new();
        for (name, entry_inode) in entry_list {
            if let Some(entry_inode_data) = inodes.get(&entry_inode) {
                entries.push(DirectoryEntry {
                    name,
                    inode: entry_inode,
                    file_type: entry_inode_data.metadata.file_type,
                });
            }
        }

        Ok(entries)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        // Resolve old file
        let old_inode = self.resolve_path(old_path)?;
        let (old_parent_inode, old_filename) = self.resolve_parent(old_path)?;
        let (new_parent_inode, new_filename) = self.resolve_parent(new_path)?;

        if new_filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }

        let mut inodes = self.inodes.write();

        // Check if new file already exists
        let new_parent = inodes.get(&new_parent_inode).ok_or(FsError::NotFound)?;
        if new_parent.entries.contains_key(&new_filename) {
            return Err(FsError::AlreadyExists);
        }

        // Remove from old parent
        let old_parent = inodes.get_mut(&old_parent_inode).ok_or(FsError::NotFound)?;
        old_parent.entries.remove(&old_filename);
        old_parent.metadata.modified = get_current_time();

        // Add to new parent
        let new_parent = inodes.get_mut(&new_parent_inode).ok_or(FsError::NotFound)?;
        new_parent.entries.insert(new_filename, old_inode);
        new_parent.metadata.modified = get_current_time();

        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_inode, linkname) = self.resolve_parent(link_path)?;

        if linkname.len() > 255 {
            return Err(FsError::NameTooLong);
        }

        let mut inodes = self.inodes.write();

        // Check inode limit first
        if inodes.len() >= MAX_FILES as usize {
            return Err(FsError::NoSpaceLeft);
        }

        // Check if parent exists and is a directory
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        // Check if link already exists
        if parent.entries.contains_key(&linkname) {
            return Err(FsError::AlreadyExists);
        }

        // Create new symlink inode
        let new_inode = self.allocate_inode();
        let symlink_inode =
            RamInode::new_symlink(new_inode, target, FilePermissions::from_octal(0o777));

        // Add to parent directory
        parent.entries.insert(linkname, new_inode);
        parent.metadata.modified = get_current_time();

        // Insert new inode
        inodes.insert(new_inode, symlink_inode);

        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let link_inode = self.resolve_path(path)?;
        let inodes = self.inodes.read();
        let symlink = inodes.get(&link_inode).ok_or(FsError::NotFound)?;

        if symlink.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }

        symlink.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        // RAM filesystem doesn't need syncing
        Ok(())
    }
}
