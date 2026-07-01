//! ConfigFS pseudo-filesystem for kernel configuration
//!
//! In-memory implementation of the Linux configfs virtual filesystem. It
//! maintains a tree of config items (directories), each of which can carry a
//! set of named attributes (regular files backed by an in-memory byte buffer).
//! Groups are represented as directories, supporting an arbitrary hierarchy
//! that can be created, walked, and modified through the standard `FileSystem`
//! trait.

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

/// Maximum number of config items (inodes) the tree will hold.
const MAX_CONFIG_ITEMS: u64 = 4096;

/// A config item is either a group (directory) or an attribute (file).
#[derive(Debug, Clone)]
struct ConfigItem {
    metadata: FileMetadata,
    /// Attribute payload for regular files.
    content: Vec<u8>,
    /// Child entries for groups/directories.
    entries: BTreeMap<String, InodeNumber>,
    /// Symbolic link target (configfs supports symlink between items).
    symlink_target: Option<String>,
}

impl ConfigItem {
    fn new_attribute(inode: InodeNumber, permissions: FilePermissions) -> Self {
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

    fn new_group(inode: InodeNumber, permissions: FilePermissions) -> Self {
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

/// ConfigFS filesystem — an in-memory tree of config groups and attributes.
#[derive(Debug)]
pub struct ConfigFs {
    /// All config items keyed by inode number.
    items: RwLock<BTreeMap<InodeNumber, ConfigItem>>,
    /// Next inode number to allocate.
    next_inode: RwLock<InodeNumber>,
    /// Root group inode (the configfs mount point).
    root_inode: InodeNumber,
}

impl ConfigFs {
    /// Create a new ConfigFS with an empty root group.
    pub fn new() -> Self {
        let root_inode = 1;
        let mut items = BTreeMap::new();
        let mut root = ConfigItem::new_group(root_inode, FilePermissions::default_directory());
        root.entries.insert("..".to_string(), root_inode);
        items.insert(root_inode, root);
        Self {
            items: RwLock::new(items),
            next_inode: RwLock::new(2),
            root_inode,
        }
    }

    /// Register a new config group (directory) under `parent_path`.
    pub fn create_group(&self, parent_path: &str, name: &str) -> FsResult<InodeNumber> {
        let path = if parent_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent_path.trim_end_matches('/'), name)
        };
        self.mkdir(&path, FilePermissions::default_directory())
    }

    /// Register a new attribute (file) under `parent_path`.
    pub fn create_attribute(
        &self,
        parent_path: &str,
        name: &str,
        permissions: FilePermissions,
    ) -> FsResult<InodeNumber> {
        let path = if parent_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent_path.trim_end_matches('/'), name)
        };
        self.create(&path, permissions)
    }

    /// Read the raw value of an attribute by path.
    pub fn read_attribute(&self, path: &str) -> FsResult<Vec<u8>> {
        let inode = self.open(path, OpenFlags::read_only())?;
        let items = self.items.read();
        let item = items.get(&inode).ok_or(FsError::NotFound)?;
        Ok(item.content.clone())
    }

    /// Write a raw value to an attribute by path.
    pub fn write_attribute(&self, path: &str, value: &[u8]) -> FsResult<()> {
        let inode = self.open(path, OpenFlags::read_write())?;
        let mut items = self.items.write();
        let item = items.get_mut(&inode).ok_or(FsError::NotFound)?;
        if item.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        item.content.clear();
        item.content.extend_from_slice(value);
        item.metadata.size = value.len() as u64;
        item.metadata.modified = get_current_time();
        Ok(())
    }

    /// Recursively enumerate every item beneath `path` (depth-first).
    pub fn enumerate_tree(&self, path: &str) -> FsResult<Vec<(String, FileType)>> {
        let start = self.resolve_path(path)?;
        let mut out = Vec::new();
        self.walk(start, path, &mut out);
        Ok(out)
    }

    fn walk(&self, inode: InodeNumber, prefix: &str, out: &mut Vec<(String, FileType)>) {
        // Snapshot this node's type and (if a group) its children while holding
        // the read lock, then release before recursing to avoid re-entrancy.
        let (file_type, children): (FileType, Vec<(String, InodeNumber)>) = {
            let items = self.items.read();
            let Some(item) = items.get(&inode) else {
                return;
            };
            let kids = if item.metadata.file_type == FileType::Directory {
                item.entries
                    .iter()
                    .filter(|(n, _)| n.as_str() != "." && n.as_str() != "..")
                    .map(|(n, &i)| (n.clone(), i))
                    .collect()
            } else {
                Vec::new()
            };
            (item.metadata.file_type, kids)
        };
        out.push((prefix.to_string(), file_type));
        for (name, child) in children {
            let child_path = if prefix == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", prefix, name)
            };
            self.walk(child, &child_path, out);
        }
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
        let items = self.items.read();
        let mut current = self.root_inode;
        for component in components {
            let item = items.get(&current).ok_or(FsError::NotFound)?;
            if item.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            current = *item.entries.get(&component).ok_or(FsError::NotFound)?;
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

    fn is_group_empty(&self, inode: InodeNumber) -> FsResult<bool> {
        let items = self.items.read();
        let group = items.get(&inode).ok_or(FsError::NotFound)?;
        if group.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        Ok(group.entries.len() <= 2)
    }
}

impl Default for ConfigFs {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ConfigFs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "configfs")
    }
}

impl FileSystem for ConfigFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::SysFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let items = self.items.read();
        let used = items.len() as u64;
        let block_size = 4096u32;
        let used_blocks: u64 = items
            .values()
            .map(|i| (i.content.len() as u64 + block_size as u64 - 1) / block_size as u64)
            .sum();
        let total_blocks = (MAX_CONFIG_ITEMS * block_size as u64) / block_size as u64;
        let free_blocks = total_blocks.saturating_sub(used_blocks);
        Ok(FileSystemStats {
            total_blocks,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: MAX_CONFIG_ITEMS,
            free_inodes: MAX_CONFIG_ITEMS.saturating_sub(used),
            block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, filename) = self.resolve_parent(path)?;
        if filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut items = self.items.write();
        if items.len() >= MAX_CONFIG_ITEMS as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = items.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let attr = ConfigItem::new_attribute(new_inode, permissions);
        parent.entries.insert(filename, new_inode);
        parent.metadata.modified = get_current_time();
        items.insert(new_inode, attr);
        Ok(new_inode)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut items = self.items.write();
        let item = items.get_mut(&inode).ok_or(FsError::NotFound)?;
        if item.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        item.metadata.accessed = get_current_time();
        let len = item.content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = cmp::min(start + buffer.len(), item.content.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&item.content[start..end]);
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut items = self.items.write();
        let item = items.get_mut(&inode).ok_or(FsError::NotFound)?;
        if item.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        let required = (offset + buffer.len() as u64) as usize;
        if item.content.len() < required {
            item.content.resize(required, 0);
        }
        let start = offset as usize;
        let end = start + buffer.len();
        item.content[start..end].copy_from_slice(buffer);
        item.metadata.size = item.content.len() as u64;
        item.metadata.modified = get_current_time();
        item.metadata.accessed = get_current_time();
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let items = self.items.read();
        let item = items.get(&inode).ok_or(FsError::NotFound)?;
        Ok(item.metadata.clone())
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut items = self.items.write();
        let item = items.get_mut(&inode).ok_or(FsError::NotFound)?;
        item.metadata.permissions = metadata.permissions;
        item.metadata.uid = metadata.uid;
        item.metadata.gid = metadata.gid;
        item.metadata.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        if dirname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut items = self.items.write();
        if items.len() >= MAX_CONFIG_ITEMS as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = items.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let mut group = ConfigItem::new_group(new_inode, permissions);
        group.entries.insert("..".to_string(), parent_inode);
        parent.entries.insert(dirname, new_inode);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count += 1;
        items.insert(new_inode, group);
        Ok(new_inode)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        if path == "/" {
            return Err(FsError::PermissionDenied);
        }
        let dir_inode = self.resolve_path(path)?;
        if !self.is_group_empty(dir_inode)? {
            return Err(FsError::DirectoryNotEmpty);
        }
        let (parent_inode, dirname) = self.resolve_parent(path)?;
        let mut items = self.items.write();
        let parent = items.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&dirname);
        parent.metadata.modified = get_current_time();
        parent.metadata.link_count -= 1;
        items.remove(&dir_inode);
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let file_inode = self.resolve_path(path)?;
        let (parent_inode, filename) = self.resolve_parent(path)?;
        let mut items = self.items.write();
        let item = items.get(&file_inode).ok_or(FsError::NotFound)?;
        if item.metadata.file_type == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let parent = items.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.remove(&filename);
        parent.metadata.modified = get_current_time();
        items.remove(&file_inode);
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let mut items = self.items.write();
        let group = items.get_mut(&inode).ok_or(FsError::NotFound)?;
        if group.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        group.metadata.accessed = get_current_time();
        let entry_list: Vec<(String, InodeNumber)> = group
            .entries
            .iter()
            .map(|(name, &ino)| (name.clone(), ino))
            .collect();
        let mut entries = Vec::new();
        for (name, entry_inode) in entry_list {
            if let Some(node) = items.get(&entry_inode) {
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
        let mut items = self.items.write();
        let new_parent = items.get(&new_parent_inode).ok_or(FsError::NotFound)?;
        if new_parent.entries.contains_key(&new_filename) {
            return Err(FsError::AlreadyExists);
        }
        let old_parent = items.get_mut(&old_parent_inode).ok_or(FsError::NotFound)?;
        old_parent.entries.remove(&old_filename);
        old_parent.metadata.modified = get_current_time();
        let new_parent = items.get_mut(&new_parent_inode).ok_or(FsError::NotFound)?;
        new_parent.entries.insert(new_filename, old_inode);
        new_parent.metadata.modified = get_current_time();
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_inode, linkname) = self.resolve_parent(link_path)?;
        if linkname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let mut items = self.items.write();
        if items.len() >= MAX_CONFIG_ITEMS as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = items.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if parent.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&linkname) {
            return Err(FsError::AlreadyExists);
        }
        let new_inode = self.allocate_inode();
        let sym = ConfigItem::new_symlink(new_inode, target, FilePermissions::from_octal(0o777));
        parent.entries.insert(linkname, new_inode);
        parent.metadata.modified = get_current_time();
        items.insert(new_inode, sym);
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let link_inode = self.resolve_path(path)?;
        let items = self.items.read();
        let sym = items.get(&link_inode).ok_or(FsError::NotFound)?;
        if sym.metadata.file_type != FileType::SymbolicLink {
            return Err(FsError::InvalidArgument);
        }
        sym.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
