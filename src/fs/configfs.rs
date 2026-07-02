//! configfs virtual filesystem implementation
//!
//! configfs is a callback-driven pseudo-filesystem whose entities are
//! created and destroyed through filesystem operations. Subsystems
//! register themselves with `register_subsystem`; userspace creates
//! config items by `mkdir`-ing inside a subsystem directory, which
//! invokes the subsystem's `make_item` / `make_group` callback. The
//! returned descriptor populates the new directory with attribute
//! files (each optionally carrying read/write callbacks). Removing a
//! directory with `rmdir` recursively drops the item.
//!
//! Public API:
//! - `register_subsystem(descriptor)` — register a subsystem under `/`
//! - `unregister_subsystem(name)` — remove a subsystem
//!
//! `create` returns `NotSupported` (attributes are defined by subsystems);
//! `rename`, `symlink`, and `readlink` are unsupported.

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
use core::fmt;
use spin::RwLock;

/// Root directory inode.
const ROOT_INODE: InodeNumber = 1;

/// Read callback: invoked on `read()`, returns content bytes.
pub type ConfigReadCallback = fn() -> FsResult<Vec<u8>>;

/// Write callback: invoked on `write()`, receives the bytes.
pub type ConfigWriteCallback = fn(&[u8]) -> FsResult<()>;

/// Callback to create a new config item (a directory) inside a subsystem
/// or group. Returns the attribute/filespec for the new item.
pub type ConfigMakeItem = fn(name: &str) -> FsResult<ConfigItemSpec>;

/// Callback to create a new config group (a directory) inside a subsystem
/// or group. Returns the attribute/filespec for the new group.
pub type ConfigMakeGroup = fn(name: &str) -> FsResult<ConfigItemSpec>;

/// Specification for a single attribute file within a config item.
#[derive(Debug, Default)]
pub struct ConfigAttributeSpec {
    /// Attribute file name.
    pub name: String,
    /// Optional read callback.
    pub read_callback: Option<ConfigReadCallback>,
    /// Optional write callback.
    pub write_callback: Option<ConfigWriteCallback>,
    /// Initial cached content.
    pub content: Vec<u8>,
}

/// Specification for a newly created config item or group.
#[derive(Debug, Default)]
pub struct ConfigItemSpec {
    /// Attribute files to populate the new directory with.
    pub attributes: Vec<ConfigAttributeSpec>,
    /// Optional callback for creating child items (mkdir).
    pub make_item: Option<ConfigMakeItem>,
    /// Optional callback for creating child groups (mkdir).
    pub make_group: Option<ConfigMakeGroup>,
}

/// Descriptor for a subsystem registered under the configfs root.
#[derive(Debug, Default)]
pub struct SubsystemDescriptor {
    /// Subsystem name (becomes the top-level directory name).
    pub name: String,
    /// Callback for creating items inside this subsystem.
    pub make_item: Option<ConfigMakeItem>,
    /// Callback for creating groups inside this subsystem.
    pub make_group: Option<ConfigMakeGroup>,
    /// Default attribute files for the subsystem directory itself.
    pub attributes: Vec<ConfigAttributeSpec>,
}

/// A configfs inode.
#[derive(Debug)]
struct ConfigInode {
    metadata: FileMetadata,
    children: BTreeMap<String, InodeNumber>,
    content: Vec<u8>,
    read_callback: Option<ConfigReadCallback>,
    write_callback: Option<ConfigWriteCallback>,
    /// For directories: callback to create child items via mkdir.
    make_item: Option<ConfigMakeItem>,
    /// For directories: callback to create child groups via mkdir.
    make_group: Option<ConfigMakeGroup>,
    /// True for subsystem root directories (cannot be rmdir'd).
    is_subsystem: bool,
}

impl ConfigInode {
    fn new_dir(inode: InodeNumber, permissions: FilePermissions, is_subsystem: bool) -> Self {
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
            make_item: None,
            make_group: None,
            is_subsystem,
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
            make_item: None,
            make_group: None,
            is_subsystem: false,
        }
    }
}

/// configfs filesystem instance.
#[derive(Debug)]
pub struct ConfigFs {
    inodes: RwLock<BTreeMap<InodeNumber, ConfigInode>>,
    next_inode: RwLock<InodeNumber>,
}

impl ConfigFs {
    /// Create a new, empty configfs with a root directory.
    pub fn new() -> Self {
        let mut inodes = BTreeMap::new();
        let mut root = ConfigInode::new_dir(ROOT_INODE, FilePermissions::default_directory(), false);
        root.children.insert("..".to_string(), ROOT_INODE);
        inodes.insert(ROOT_INODE, root);
        Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
        }
    }

    fn alloc_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;
        inode
    }

    /// Resolve a path to an inode number.
    fn resolve(&self, path: &str) -> FsResult<InodeNumber> {
        let inodes = self.inodes.read();
        let mut current = ROOT_INODE;
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
            return Ok((ROOT_INODE, name));
        }
        let parent_path = format!("/{}", parts[..parts.len() - 1].join("/"));
        Ok((self.resolve(&parent_path)?, name))
    }

    /// Populate a freshly created directory inode with attribute files
    /// described by a `ConfigItemSpec`. Returns the new directory inode.
    fn populate_item(&self, parent: InodeNumber, name: &str, spec: ConfigItemSpec) -> FsResult<InodeNumber> {
        let inode = self.alloc_inode();
        let mut dir = ConfigInode::new_dir(inode, FilePermissions::default_directory(), false);
        dir.children.insert("..".to_string(), parent);
        dir.make_item = spec.make_item;
        dir.make_group = spec.make_group;

        let mut inodes = self.inodes.write();
        // Insert the directory first.
        inodes.insert(inode, dir);

        // Create attribute files.
        for attr in spec.attributes {
            let attr_inode = self.alloc_inode();
            let mut file = ConfigInode::new_file(attr_inode, FilePermissions::from_octal(0o644));
            file.content = attr.content.clone();
            file.read_callback = attr.read_callback;
            file.write_callback = attr.write_callback;
            file.metadata.size = file.content.len() as u64;
            inodes.insert(attr_inode, file);
            let dir = inodes.get_mut(&inode).unwrap();
            dir.children.insert(attr.name, attr_inode);
        }

        // Link the new directory into its parent.
        let parent_node = inodes.get_mut(&parent).ok_or(FsError::NotFound)?;
        parent_node.children.insert(name.to_string(), inode);
        parent_node.metadata.link_count += 1;
        parent_node.metadata.modified = get_current_time();
        Ok(inode)
    }

    /// Register a subsystem under the configfs root.
    ///
    /// Creates a top-level directory named `descriptor.name` populated with
    /// the subsystem's default attributes and item/group creation callbacks.
    pub fn register_subsystem(&self, descriptor: SubsystemDescriptor) -> FsResult<InodeNumber> {
        if descriptor.name.is_empty() || descriptor.name.len() > 255 {
            return Err(FsError::InvalidArgument);
        }
        let inode = self.alloc_inode();
        let mut dir = ConfigInode::new_dir(inode, FilePermissions::default_directory(), true);
        dir.children.insert("..".to_string(), ROOT_INODE);
        dir.make_item = descriptor.make_item;
        dir.make_group = descriptor.make_group;

        let mut inodes = self.inodes.write();
        if let Some(root) = inodes.get(&ROOT_INODE) {
            if root.children.contains_key(&descriptor.name) {
                return Err(FsError::AlreadyExists);
            }
        }
        inodes.insert(inode, dir);

        // Create subsystem default attribute files.
        for attr in descriptor.attributes {
            let attr_inode = self.alloc_inode();
            let mut file = ConfigInode::new_file(attr_inode, FilePermissions::from_octal(0o644));
            file.content = attr.content.clone();
            file.read_callback = attr.read_callback;
            file.write_callback = attr.write_callback;
            file.metadata.size = file.content.len() as u64;
            inodes.insert(attr_inode, file);
            let dir = inodes.get_mut(&inode).unwrap();
            dir.children.insert(attr.name, attr_inode);
        }

        // Link into root.
        let root = inodes.get_mut(&ROOT_INODE).unwrap();
        root.children.insert(descriptor.name, inode);
        root.metadata.link_count += 1;
        root.metadata.modified = get_current_time();
        Ok(inode)
    }

    /// Unregister a subsystem by name, recursively removing it.
    pub fn unregister_subsystem(&self, name: &str) -> FsResult<()> {
        let path = format!("/{}", name);
        let target = self.resolve(&path)?;
        {
            let inodes = self.inodes.read();
            let node = inodes.get(&target).ok_or(FsError::NotFound)?;
            if !node.is_subsystem {
                return Err(FsError::InvalidArgument);
            }
        }
        // Remove from root then recursively drop.
        let mut inodes = self.inodes.write();
        let root = inodes.get_mut(&ROOT_INODE).unwrap();
        root.children.remove(name);
        root.metadata.modified = get_current_time();
        Self::remove_recursive(&mut inodes, target);
        Ok(())
    }

    /// Recursively remove an inode and all its descendants.
    fn remove_recursive(inodes: &mut BTreeMap<InodeNumber, ConfigInode>, inode: InodeNumber) {
        if let Some(node) = inodes.remove(&inode) {
            for (_, child) in node.children.iter() {
                if *child != inode {
                    Self::remove_recursive(inodes, *child);
                }
            }
        }
    }
}

impl fmt::Display for ConfigFs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "configfs")
    }
}

impl FileSystem for ConfigFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::ConfigFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
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

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // Attributes are defined by subsystems; plain file creation is not
        // supported through the VFS.
        Err(FsError::NotSupported)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        // Snapshot content (from callback or cache) without holding the
        // lock while invoking the callback.
        let content: Vec<u8> = {
            let inodes = self.inodes.read();
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
        let mut inodes = self.inodes.write();
        if let Some(node) = inodes.get_mut(&inode) {
            node.metadata.accessed = get_current_time();
        }
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        // Invoke the write callback outside the lock if present.
        let has_cb = {
            let inodes = self.inodes.read();
            let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
            if node.metadata.file_type != FileType::Regular {
                return Err(FsError::IsADirectory);
            }
            node.write_callback.is_some()
        };

        if has_cb {
            let cb = {
                let inodes = self.inodes.read();
                inodes.get(&inode).and_then(|n| n.write_callback)
            };
            if let Some(cb) = cb {
                cb(buffer)?;
            }
        }

        // Persist bytes into stored content.
        let mut inodes = self.inodes.write();
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
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        Ok(node.metadata.clone())
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.metadata.permissions = metadata.permissions;
        node.metadata.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, name) = self.resolve_parent(path)?;
        if name.is_empty() || name.len() > 255 {
            return Err(FsError::NameTooLong);
        }

        // Determine which creation callback the parent supports. Prefer
        // make_group for nested groups, fall back to make_item.
        let make_cb: Option<ConfigMakeItem> = {
            let inodes = self.inodes.read();
            let parent = inodes.get(&parent_inode).ok_or(FsError::NotFound)?;
            if parent.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            if parent.children.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
            parent.make_item.or(parent.make_group)
        };

        let spec = match make_cb {
            Some(cb) => cb(&name)?,
            None => ConfigItemSpec::default(),
        };

        self.populate_item(parent_inode, &name, spec)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let target = self.resolve(path)?;
        if target == ROOT_INODE {
            return Err(FsError::PermissionDenied);
        }
        let (parent_inode, name) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();

        // Subsystem directories can only be removed via unregister_subsystem.
        let node = inodes.get(&target).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if node.is_subsystem {
            return Err(FsError::PermissionDenied);
        }

        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.children.remove(&name).is_none() {
            return Err(FsError::NotFound);
        }
        parent.metadata.modified = get_current_time();

        Self::remove_recursive(&mut inodes, target);
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        // Attribute files are managed by their parent item; direct unlink
        // is not part of the configfs model.
        let target = self.resolve(path)?;
        let inodes = self.inodes.read();
        let node = inodes.get(&target).ok_or(FsError::NotFound)?;
        if node.metadata.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let _ = node;
        // Removing an attribute file is allowed only if it is not a
        // subsystem-managed attribute; we permit it as a drop operation.
        drop(inodes);
        let (parent_inode, name) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.children.remove(&name).is_none() {
            return Err(FsError::NotFound);
        }
        parent.metadata.modified = get_current_time();
        Self::remove_recursive(&mut inodes, target);
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inodes = self.inodes.read();
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
