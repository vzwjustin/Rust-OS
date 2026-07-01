//! ExportFS utilities for file handle encoding/decoding
//!
//! In-memory implementation of the Linux exportfs subsystem. It tracks the
//! set of exported paths, maintains a bidirectional mapping between file
//! handles (opaque 64-bit tokens) and filesystem paths, and records per-export
//! flags. The backing store is a simple in-memory VFS tree so that handle
//! encoding/decoding can be exercised end-to-end without a block device.

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
use core::fmt;
use spin::RwLock;

/// Maximum number of inodes the in-memory exportfs tree will hold.
const MAX_EXPORT_INODES: u64 = 4096;

bitflags::bitflags! {
    /// Flags controlling an individual export entry, mirroring the Linux
    /// `struct export_operations` / `exp_flags` concepts.
    pub struct ExportFlags: u32 {
        /// Export is readable by unprivileged clients.
        const READABLE    = 0b0000_0001;
        /// Export is writable.
        const WRITABLE    = 0b0000_0010;
        /// Cross-mountpoint lookups are allowed.
        const CROSS_MOUNT = 0b0000_0100;
        /// File handles may be persisted across remounts.
        const PERSISTENT  = 0b0000_1000;
        /// Anonymous (no-path) handles are permitted.
        const ANONYMOUS   = 0b0001_0000;
    }
}

/// A file handle is an opaque 64-bit token that maps back to a path.
pub type FileHandle = u64;

/// Per-export tracking record.
#[derive(Debug, Clone)]
struct ExportEntry {
    /// The exported filesystem path (always begins with '/').
    path: String,
    /// Flags applied to this export.
    flags: ExportFlags,
    /// Next handle value to hand out for this export.
    next_handle: FileHandle,
    /// Bidirectional mapping: handle <-> path (relative to the export root).
    handle_to_path: BTreeMap<FileHandle, String>,
    path_to_handle: BTreeMap<String, FileHandle>,
}

/// In-memory inode for the backing VFS tree.
#[derive(Debug, Clone)]
struct ExportInode {
    metadata: FileMetadata,
    content: Vec<u8>,
    entries: BTreeMap<String, InodeNumber>,
    symlink_target: Option<String>,
}

impl ExportInode {
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
            content: Vec::new(),
            entries,
            symlink_target: None,
        }
    }

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

/// ExportFS filesystem — an in-memory VFS with NFS-style export tracking.
#[derive(Debug)]
pub struct ExportFs {
    /// All inodes in the backing tree.
    inodes: RwLock<BTreeMap<InodeNumber, ExportInode>>,
    /// Next inode number to allocate.
    next_inode: RwLock<InodeNumber>,
    /// Root directory inode.
    root_inode: InodeNumber,
    /// Registered exports keyed by export root path.
    exports: RwLock<BTreeMap<String, ExportEntry>>,
}

impl ExportFs {
    /// Create a new ExportFS with an empty root directory and no exports.
    pub fn new() -> Self {
        let root_inode = 1;
        let mut inodes = BTreeMap::new();
        let mut root = ExportInode::new_directory(root_inode, FilePermissions::default_directory());
        root.entries.insert("..".to_string(), root_inode);
        inodes.insert(root_inode, root);

        Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
            root_inode,
            exports: RwLock::new(BTreeMap::new()),
        }
    }

    /// Register a path as exported with the given flags.
    pub fn export_path(&self, path: &str, flags: ExportFlags) -> FsResult<()> {
        if !path.starts_with('/') {
            return Err(FsError::InvalidArgument);
        }
        // The path must resolve to an existing directory in the tree.
        let _ = self.resolve_path(path)?;
        let mut exports = self.exports.write();
        exports.insert(
            path.to_string(),
            ExportEntry {
                path: path.to_string(),
                flags,
                next_handle: 1,
                handle_to_path: BTreeMap::new(),
                path_to_handle: BTreeMap::new(),
            },
        );
        Ok(())
    }

    /// Remove an export registration.
    pub fn unexport_path(&self, path: &str) -> FsResult<()> {
        let mut exports = self.exports.write();
        if exports.remove(path).is_some() {
            Ok(())
        } else {
            Err(FsError::NotFound)
        }
    }

    /// Encode a path into a persistent file handle for a given export root.
    pub fn encode_handle(&self, export_root: &str, path: &str) -> FsResult<FileHandle> {
        let full_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("{}/{}", export_root.trim_end_matches('/'), path)
        };
        // Verify the path exists.
        let _ = self.resolve_path(&full_path)?;

        let mut exports = self.exports.write();
        let entry = exports.get_mut(export_root).ok_or(FsError::NotFound)?;
        if !entry.flags.contains(ExportFlags::PERSISTENT)
            && !entry.flags.contains(ExportFlags::ANONYMOUS)
        {
            return Err(FsError::NotSupported);
        }

        let rel = full_path
            .strip_prefix(export_root)
            .unwrap_or(&full_path)
            .trim_start_matches('/');
        let rel_key = if rel.is_empty() { "/" } else { rel };
        if let Some(&h) = entry.path_to_handle.get(rel_key) {
            return Ok(h);
        }
        let handle = entry.next_handle;
        entry.next_handle += 1;
        entry.handle_to_path.insert(handle, rel_key.to_string());
        entry.path_to_handle.insert(rel_key.to_string(), handle);
        Ok(handle)
    }

    /// Decode a file handle back to a full filesystem path.
    pub fn decode_handle(&self, export_root: &str, handle: FileHandle) -> FsResult<String> {
        let exports = self.exports.read();
        let entry = exports.get(export_root).ok_or(FsError::NotFound)?;
        let rel = entry.handle_to_path.get(&handle).ok_or(FsError::NotFound)?;
        if rel == "/" {
            Ok(export_root.to_string())
        } else {
            Ok(format!("{}/{}", export_root.trim_end_matches('/'), rel))
        }
    }

    /// Query the flags associated with an export.
    pub fn export_flags(&self, export_root: &str) -> FsResult<ExportFlags> {
        self.exports
            .read()
            .get(export_root)
            .map(|e| e.flags)
            .ok_or(FsError::NotFound)
    }

    /// List all currently exported root paths.
    pub fn list_exports(&self) -> Vec<String> {
        self.exports.read().keys().cloned().collect()
    }

    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;
        inode
    }

    fn split_path(&self, path: &str) -> Vec<String> {
        path.split('/')
            .filter(|c| !c.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(self.root_inode);
        }
        let components = self.split_path(path);
        let inodes = self.inodes.read();
        let mut current = self.root_inode;
        for component in components {
            let inode = inodes.get(&current).ok_or(FsError::NotFound)?;
            if inode.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            current = *inode.entries.get(&component).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }

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
            Ok((self.root_inode, filename))
        } else {
            let parent_path = format!("/{}", components[..components.len() - 1].join("/"));
            let parent_inode = self.resolve_path(&parent_path)?;
            Ok((parent_inode, filename))
        }
    }

    fn is_directory_empty(&self, inode: InodeNumber) -> FsResult<bool> {
        let inodes = self.inodes.read();
        let dir = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        Ok(dir.entries.len() <= 2)
    }
}

impl Default for ExportFs {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ExportFs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "exportfs")
    }
}

impl FileSystem for ExportFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::RamFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let used = inodes.len() as u64;
        let block_size = 4096u32;
        let used_blocks: u64 = inodes
            .values()
            .map(|i| (i.content.len() as u64 + block_size as u64 - 1) / block_size as u64)
            .sum();
        let total_blocks = (MAX_EXPORT_INODES * block_size as u64) / block_size as u64;
        let free_blocks = total_blocks.saturating_sub(used_blocks);
        Ok(FileSystemStats {
            total_blocks,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: MAX_EXPORT_INODES,
            free_inodes: MAX_EXPORT_INODES.saturating_sub(used),
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
        if inodes.len() >= MAX_EXPORT_INODES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let file_inode = ExportInode::new_file(new_inode, permissions);
        parent.entries.insert(filename, new_inode);
        parent.metadata.modified = get_current_time();
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
        file_inode.metadata.accessed = get_current_time();
        let len = file_inode.content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = cmp::min(start + buffer.len(), file_inode.content.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&file_inode.content[start..end]);
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let file_inode = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if file_inode.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        let required = (offset + buffer.len() as u64) as usize;
        if file_inode.content.len() < required {
            file_inode.content.resize(required, 0);
        }
        let start = offset as usize;
        let end = start + buffer.len();
        file_inode.content[start..end].copy_from_slice(buffer);
        file_inode.metadata.size = file_inode.content.len() as u64;
        file_inode.metadata.modified = get_current_time();
        file_inode.metadata.accessed = get_current_time();
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
        if inodes.len() >= MAX_EXPORT_INODES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut dir_inode = ExportInode::new_directory(new_inode, permissions);
        dir_inode.entries.insert("..".to_string(), parent_inode);
        parent.entries.insert(dirname, new_inode);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count += 1;
        inodes.insert(new_inode, dir_inode);
        Ok(new_inode)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        if path == "/" {
            return Err(FsError::PermissionDenied);
        }
        let dir_inode = self.resolve_path(path)?;
        if !self.is_directory_empty(dir_inode)? {
            return Err(FsError::DirectoryNotEmpty);
        }
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&dirname);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count -= 1;
        inodes.remove(&dir_inode);
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let file_inode = self.resolve_path(path)?;
        let (parent_inode, filename) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let file = inodes.get(&file_inode).ok_or(FsError::NotFound)?;
        if file.metadata.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&filename);
        parent.metadata.modified = get_current_time();
        inodes.remove(&file_inode);
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut inodes = self.inodes.write();
        let dir_inode = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if dir_inode.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        dir_inode.metadata.accessed = get_current_time();
        let entry_list: Vec<(String, InodeNumber)> = dir_inode
            .entries
            .iter()
            .map(|(name, &ino)| (name.clone(), ino))
            .collect();
        let mut entries = Vec::new();
        for (name, entry_inode) in entry_list {
            if let Some(node) = inodes.get(&entry_inode) {
                entries.push(DirectoryEntry {
                    name,
                    inode: entry_inode,
                    file_type: node.metadata.file_type,
                });
            }
        }
        Ok(entries)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let old_inode = self.resolve_path(old_path)?;
        let (old_parent_inode, old_filename) = self.resolve_parent(old_path)?;
        let (new_parent_inode, new_filename) = self.resolve_parent(new_path)?;
        if new_filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut inodes = self.inodes.write();
        let new_parent = inodes.get(&new_parent_inode).ok_or(FsError::NotFound)?;
        if new_parent.entries.contains_key(&new_filename) {
            return Err(FsError::AlreadyExists);
        }
        let old_parent = inodes.get_mut(&old_parent_inode).ok_or(FsError::NotFound)?;
        old_parent.entries.remove(&old_filename);
        old_parent.metadata.modified = get_current_time();
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
        if inodes.len() >= MAX_EXPORT_INODES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&linkname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let sym = ExportInode::new_symlink(new_inode, target, FilePermissions::from_octal(0o777));
        parent.entries.insert(linkname, new_inode);
        parent.metadata.modified = get_current_time();
        inodes.insert(new_inode, sym);
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let link_inode = self.resolve_path(path)?;
        let inodes = self.inodes.read();
        let sym = inodes.get(&link_inode).ok_or(FsError::NotFound)?;
        if sym.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        sym.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
