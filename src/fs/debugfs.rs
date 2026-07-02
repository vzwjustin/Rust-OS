//! debugfs virtual filesystem implementation
//!
//! debugfs is a callback-based pseudo-filesystem that exposes kernel
//! debugging interfaces to userspace. Files are created programmatically
//! with optional read/write callbacks; when a callback is absent the
//! cached content buffer is used instead.
//!
//! Public API:
//! - `create_file(name, parent, read_cb, write_cb)` — register a debug file
//! - `create_dir(name, parent)` — register a debug directory
//! - `remove(name)` — remove a file or directory by path

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
use spin::Mutex;

/// Read callback signature: invoked on `read()`, returns content bytes.
pub type DebugfsReadCallback = fn() -> FsResult<Vec<u8>>;

/// Write callback signature: invoked on `write()`, receives the bytes.
pub type DebugfsWriteCallback = fn(&[u8]) -> FsResult<()>;

/// A debugfs inode.
#[derive(Debug)]
struct DebugfsInode {
    /// File metadata.
    metadata: FileMetadata,
    /// Child entries for directories (name -> inode).
    children: BTreeMap<String, InodeNumber>,
    /// Cached content for regular files without a read callback.
    content: Vec<u8>,
    /// Optional read callback.
    read_callback: Option<DebugfsReadCallback>,
    /// Optional write callback.
    write_callback: Option<DebugfsWriteCallback>,
}

impl DebugfsInode {
    fn new_dir(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let mut children = BTreeMap::new();
        children.insert(".".to_string(), inode);
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
            children,
            content: Vec::new(),
            read_callback: None,
            write_callback: None,
        }
    }

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
            children: BTreeMap::new(),
            content: Vec::new(),
            read_callback: None,
            write_callback: None,
        }
    }
}

/// debugfs filesystem instance.
#[derive(Debug)]
pub struct DebugfsFileSystem {
    inodes: Mutex<BTreeMap<InodeNumber, DebugfsInode>>,
    next_inode: Mutex<InodeNumber>,
}

impl DebugfsFileSystem {
    /// Root directory inode.
    const ROOT_INODE: InodeNumber = 1;

    /// Create a new debugfs instance with an empty root directory.
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        let mut root = DebugfsInode::new_dir(Self::ROOT_INODE, FilePermissions::default_directory());
        root.children
            .insert("..".to_string(), Self::ROOT_INODE);
        inodes.insert(Self::ROOT_INODE, root);
        Ok(Self {
            inodes: Mutex::new(inodes),
            next_inode: Mutex::new(2),
        })
    }

    fn alloc_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.lock();
        let inode = *next;
        *next += 1;
        inode
    }

    /// Resolve a path to an inode number.
    fn resolve(&self, path: &str) -> FsResult<InodeNumber> {
        let inodes = self.inodes.lock();
        let mut current = Self::ROOT_INODE;
        for part in path.split('/').filter(|s| !s.is_empty()) {
            let node = inodes.get(&current).ok_or(FsError::NotFound)?;
            if node.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            current = *node.children.get(part).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }

    /// Resolve the parent directory inode and the trailing name.
    fn resolve_parent(&self, path: &str) -> FsResult<(InodeNumber, String)> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        let name = parts.last().unwrap().to_string();
        if parts.len() == 1 {
            return Ok((Self::ROOT_INODE, name));
        }
        let parent_path = format!("/{}", parts[..parts.len() - 1].join("/"));
        Ok((self.resolve(&parent_path)?, name))
    }

    /// Create a debug file under `parent` with optional callbacks.
    ///
    /// `parent` is an inode number (use `ROOT_INODE` for top-level files).
    pub fn create_file(
        &self,
        name: &str,
        parent: InodeNumber,
        read_cb: Option<DebugfsReadCallback>,
        write_cb: Option<DebugfsWriteCallback>,
    ) -> FsResult<InodeNumber> {
        if name.is_empty() || name.len() > 255 {
            return Err(FsError::InvalidArgument);
        }
        let mut inodes = self.inodes.lock();
        let parent_node = inodes.get(&parent).ok_or(FsError::NotFound)?;
        if parent_node.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent_node.children.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }

        let inode = self.alloc_inode();
        let mut file = DebugfsInode::new_file(inode, FilePermissions::from_octal(0o644));
        file.read_callback = read_cb;
        file.write_callback = write_cb;
        inodes.insert(inode, file);

        let parent_node = inodes.get_mut(&parent).unwrap();
        parent_node.children.insert(name.to_string(), inode);
        parent_node.metadata.modified = get_current_time();
        Ok(inode)
    }

    /// Create a debug directory under `parent`.
    pub fn create_dir(&self, name: &str, parent: InodeNumber) -> FsResult<InodeNumber> {
        if name.is_empty() || name.len() > 255 {
            return Err(FsError::InvalidArgument);
        }
        let mut inodes = self.inodes.lock();
        let parent_node = inodes.get(&parent).ok_or(FsError::NotFound)?;
        if parent_node.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent_node.children.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }

        let inode = self.alloc_inode();
        let mut dir = DebugfsInode::new_dir(inode, FilePermissions::default_directory());
        dir.children.insert("..".to_string(), parent);
        inodes.insert(inode, dir);

        let parent_node = inodes.get_mut(&parent).unwrap();
        parent_node.children.insert(name.to_string(), inode);
        parent_node.metadata.link_count += 1;
        parent_node.metadata.modified = get_current_time();
        Ok(inode)
    }

    /// Recursively remove an inode and all its descendants.
    fn remove_recursive(inodes: &mut BTreeMap<InodeNumber, DebugfsInode>, inode: InodeNumber) {
        if let Some(node) = inodes.remove(&inode) {
            for (_, child) in node.children.iter() {
                if *child != inode {
                    Self::remove_recursive(inodes, *child);
                }
            }
        }
    }

    /// Remove a file or (recursively) a directory by path.
    pub fn remove(&self, path: &str) -> FsResult<()> {
        let target = self.resolve(path)?;
        if target == Self::ROOT_INODE {
            return Err(FsError::PermissionDenied);
        }
        let (parent_inode, name) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.lock();

        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if !parent.children.remove(&name).is_some() {
            return Err(FsError::NotFound);
        }
        parent.metadata.modified = get_current_time();

        Self::remove_recursive(&mut inodes, target);
        Ok(())
    }
}

impl FileSystem for DebugfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::DebugFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.lock();
        Ok(FileSystemStats {
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: inodes.len() as u64,
            free_inodes: 0,
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        // VFS-driven file creation (no callbacks).
        let (parent_inode, name) = self.resolve_parent(path)?;
        let inode = self.create_file(&name, parent_inode, None, None)?;
        // Apply requested permissions.
        let mut inodes = self.inodes.lock();
        if let Some(node) = inodes.get_mut(&inode) {
            node.metadata.permissions = permissions;
        }
        Ok(inode)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        // Snapshot the content (either from callback or cache) without
        // holding the lock while invoking the callback, then copy out.
        let content: Vec<u8> = {
            let inodes = self.inodes.lock();
            let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
            if node.metadata.file_type != FileType::Regular {
                return Err(FsError::IsADirectory);
            }
            if let Some(cb) = node.read_callback {
                drop(inodes);
                cb()?
            } else {
                node.content.clone()
            }
        };

        let len = content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = core::cmp::min(content.len(), start + buffer.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&content[start..end]);

        // Update access time.
        let mut inodes = self.inodes.lock();
        if let Some(node) = inodes.get_mut(&inode) {
            node.metadata.accessed = get_current_time();
        }
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        // Invoke the write callback outside the lock if present.
        let has_cb = {
            let inodes = self.inodes.lock();
            let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
            if node.metadata.file_type != FileType::Regular {
                return Err(FsError::IsADirectory);
            }
            node.write_callback.is_some()
        };

        if has_cb {
            let cb = {
                let inodes = self.inodes.lock();
                inodes.get(&inode).and_then(|n| n.write_callback)
            };
            if let Some(cb) = cb {
                cb(buffer)?;
            }
        }

        // Persist bytes into the cached content.
        let mut inodes = self.inodes.lock();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        let start = offset as usize;
        let end = start + buffer.len();
        if end > node.content.len() {
            node.content.resize(end, 0);
        }
        node.content[start..end].copy_from_slice(buffer);
        node.metadata.size = node.content.len() as u64;
        node.metadata.modified = get_current_time();
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let inodes = self.inodes.lock();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        Ok(node.metadata.clone())
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.lock();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.metadata.permissions = metadata.permissions;
        node.metadata.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, name) = self.resolve_parent(path)?;
        let inode = self.create_dir(&name, parent_inode)?;
        let mut inodes = self.inodes.lock();
        if let Some(node) = inodes.get_mut(&inode) {
            node.metadata.permissions = permissions;
        }
        Ok(inode)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let target = self.resolve(path)?;
        {
            let inodes = self.inodes.lock();
            let node = inodes.get(&target).ok_or(FsError::NotFound)?;
            if node.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            // Must be empty (only . and ..).
            if node.children.len() > 2 {
                return Err(FsError::DirectoryNotEmpty);
            }
        }
        self.remove(path)
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let target = self.resolve(path)?;
        {
            let inodes = self.inodes.lock();
            let node = inodes.get(&target).ok_or(FsError::NotFound)?;
            if node.metadata.file_type == FileType::Directory {
                return Err(FsError::IsADirectory);
            }
        }
        self.remove(path)
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inodes = self.inodes.lock();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        let mut entries = Vec::new();
        for (name, &child_inode) in node.children.iter() {
            let file_type = inodes
                .get(&child_inode)
                .map(|c| c.metadata.file_type)
                .unwrap_or(FileType::Regular);
            entries.push(DirectoryEntry {
                name: name.clone(),
                inode: child_inode,
                file_type,
            });
        }
        Ok(entries)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
