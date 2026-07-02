//! Kernfs virtual filesystem implementation
//!
//! Kernfs is the foundation for sysfs, debugfs, cgroup2, and other kernel
//! attribute filesystems. It dynamically generates kernel state as files and
//! directories, invoking registered callbacks for reads and writes.
//!
//! Design:
//! - Each node is a `KernfsNode` (dir / file / symlink) stored in a
//!   `BTreeMap<InodeNumber, KernfsNode>` under an `RwLock`.
//! - File nodes carry optional read/write callbacks. Callbacks are invoked
//!   **outside** the tree lock to avoid deadlock (a callback may itself walk
//!   the tree).
//! - The public `create_file_ns` / `create_dir_ns` / `create_link_ns` /
//!   `remove_ns` API is the primary interface used by sysfs/debugfs layers.

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
use spin::RwLock;

/// Read callback type: receives the file's cached content and returns new
/// content to present to readers. The `offset` is the read offset requested.
pub type ReadCallback = Arc<dyn Fn(&[u8], u64, usize) -> Vec<u8> + Send + Sync>;

/// Write callback type: receives the data written and the offset. Returns the
/// number of bytes consumed (usually `data.len()`).
pub type WriteCallback = Arc<dyn Fn(&mut Vec<u8>, u64, &[u8]) -> usize + Send + Sync>;

/// Type of a kernfs node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernfsNodeType {
    Directory,
    File,
    Symlink,
}

/// A single kernfs node (directory, file, or symlink).
pub struct KernfsNode {
    /// Inode number of this node.
    pub inode: InodeNumber,
    /// Node type.
    pub node_type: KernfsNodeType,
    /// Name within the parent directory.
    pub name: String,
    /// Metadata (permissions, timestamps, uid/gid, link count).
    pub metadata: FileMetadata,
    /// Parent inode number (root's parent is itself).
    pub parent: InodeNumber,
    /// Children: name -> inode number (for directories).
    pub children: BTreeMap<String, InodeNumber>,
    /// Cached file content (for file nodes).
    pub content: Vec<u8>,
    /// Read callback (for file nodes).
    pub read_cb: Option<ReadCallback>,
    /// Write callback (for file nodes).
    pub write_cb: Option<WriteCallback>,
    /// Symlink target path (for symlink nodes).
    pub symlink_target: Option<String>,
}

impl core::fmt::Debug for KernfsNode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("KernfsNode")
            .field("inode", &self.inode)
            .field("node_type", &self.node_type)
            .field("name", &self.name)
            .field("parent", &self.parent)
            .field("children_count", &self.children.len())
            .field("content_len", &self.content.len())
            .field("has_read_cb", &self.read_cb.is_some())
            .field("has_write_cb", &self.write_cb.is_some())
            .field("symlink_target", &self.symlink_target)
            .finish()
    }
}

impl KernfsNode {
    fn new_dir(inode: InodeNumber, parent: InodeNumber, name: &str, perms: FilePermissions) -> Self {
        let mut md = FileMetadata::new(inode, FileType::Directory, 0);
        md.permissions = perms;
        md.link_count = 2;
        Self {
            inode,
            node_type: KernfsNodeType::Directory,
            name: name.to_string(),
            metadata: md,
            parent,
            children: BTreeMap::new(),
            content: Vec::new(),
            read_cb: None,
            write_cb: None,
            symlink_target: None,
        }
    }

    fn new_file(inode: InodeNumber, parent: InodeNumber, name: &str, perms: FilePermissions) -> Self {
        let mut md = FileMetadata::new(inode, FileType::Regular, 0);
        md.permissions = perms;
        Self {
            inode,
            node_type: KernfsNodeType::File,
            name: name.to_string(),
            metadata: md,
            parent,
            children: BTreeMap::new(),
            content: Vec::new(),
            read_cb: None,
            write_cb: None,
            symlink_target: None,
        }
    }

    fn new_symlink(inode: InodeNumber, parent: InodeNumber, name: &str, target: &str) -> Self {
        let mut md = FileMetadata::new(inode, FileType::SymbolicLink, target.len() as u64);
        md.permissions = FilePermissions::from_octal(0o777);
        Self {
            inode,
            node_type: KernfsNodeType::Symlink,
            name: name.to_string(),
            metadata: md,
            parent,
            children: BTreeMap::new(),
            content: Vec::new(),
            read_cb: None,
            write_cb: None,
            symlink_target: Some(target.to_string()),
        }
    }
}

/// Kernfs virtual filesystem.
pub struct KernfsFileSystem {
    nodes: RwLock<BTreeMap<InodeNumber, KernfsNode>>,
    next_inode: RwLock<InodeNumber>,
    root_inode: InodeNumber,
}

impl core::fmt::Debug for KernfsFileSystem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("KernfsFileSystem")
            .field("root_inode", &self.root_inode)
            .field("next_inode", &*self.next_inode.read())
            .finish()
    }
}

impl KernfsFileSystem {
    /// Create a new kernfs instance with a root directory.
    pub fn new() -> FsResult<Self> {
        let root = 1u64;
        let mut nodes = BTreeMap::new();
        let mut root_node = KernfsNode::new_dir(root, root, "/", FilePermissions::default_directory());
        root_node.children.insert(".".to_string(), root);
        root_node.children.insert("..".to_string(), root);
        nodes.insert(root, root_node);
        Ok(Self {
            nodes: RwLock::new(nodes),
            next_inode: RwLock::new(2),
            root_inode: root,
        })
    }

    fn alloc_inode(&self) -> InodeNumber {
        let mut ni = self.next_inode.write();
        let v = *ni;
        *ni += 1;
        v
    }

    // ------------------------------------------------------------------
    // Path resolution
    // ------------------------------------------------------------------

    fn split_path(path: &str) -> Vec<String> {
        path.split('/').filter(|c| !c.is_empty()).map(|s| s.to_string()).collect()
    }

    fn resolve_parent(path: &str) -> FsResult<(Vec<String>, String)> {
        let comps = Self::split_path(path);
        if comps.is_empty() {
            return Err(FsError::InvalidArgument);
        }
        let name = comps.last().unwrap().clone();
        let parent = comps[..comps.len() - 1].to_vec();
        Ok((parent, name))
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        if path == "/" {
            return Ok(self.root_inode);
        }
        let comps = Self::split_path(path);
        let nodes = self.nodes.read();
        let mut cur = self.root_inode;
        for comp in comps {
            let node = nodes.get(&cur).ok_or(FsError::NotFound)?;
            if node.node_type != KernfsNodeType::Directory {
                return Err(FsError::NotADirectory);
            }
            cur = *node.children.get(&comp).ok_or(FsError::NotFound)?;
        }
        Ok(cur)
    }

    // ------------------------------------------------------------------
    // Public API for sysfs/debugfs layers
    // ------------------------------------------------------------------

    /// Create a file with optional read and write callbacks under `parent`.
    /// If `parent` is `None`, the root is used.
    pub fn create_file_ns(
        &self,
        name: &str,
        parent: Option<InodeNumber>,
        read_cb: Option<ReadCallback>,
        write_cb: Option<WriteCallback>,
    ) -> FsResult<InodeNumber> {
        if name.is_empty() || name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_ino = parent.unwrap_or(self.root_inode);
        let new_ino = self.alloc_inode();
        let mut nodes = self.nodes.write();
        let pnode = nodes.get_mut(&parent_ino).ok_or(FsError::NotFound)?;
        if pnode.node_type != KernfsNodeType::Directory {
            return Err(FsError::NotADirectory);
        }
        if pnode.children.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }
        let mut file_node = KernfsNode::new_file(new_ino, parent_ino, name, FilePermissions::from_octal(0o644));
        file_node.read_cb = read_cb;
        file_node.write_cb = write_cb;
        pnode.children.insert(name.to_string(), new_ino);
        pnode.metadata.modified = get_current_time();
        nodes.insert(new_ino, file_node);
        Ok(new_ino)
    }

    /// Create a directory under `parent` (root if `None`).
    pub fn create_dir_ns(
        &self,
        name: &str,
        parent: Option<InodeNumber>,
    ) -> FsResult<InodeNumber> {
        if name.is_empty() || name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_ino = parent.unwrap_or(self.root_inode);
        let new_ino = self.alloc_inode();
        let mut nodes = self.nodes.write();
        let pnode = nodes.get_mut(&parent_ino).ok_or(FsError::NotFound)?;
        if pnode.node_type != KernfsNodeType::Directory {
            return Err(FsError::NotADirectory);
        }
        if pnode.children.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }
        let mut dir_node = KernfsNode::new_dir(new_ino, parent_ino, name, FilePermissions::default_directory());
        dir_node.children.insert(".".to_string(), new_ino);
        dir_node.children.insert("..".to_string(), parent_ino);
        pnode.children.insert(name.to_string(), new_ino);
        pnode.metadata.modified = get_current_time();
        pnode.metadata.link_count = pnode.metadata.link_count.saturating_add(1);
        nodes.insert(new_ino, dir_node);
        Ok(new_ino)
    }

    /// Create a symlink `name` -> `target` under `parent`.
    pub fn create_link_ns(
        &self,
        name: &str,
        parent: Option<InodeNumber>,
        target: &str,
    ) -> FsResult<InodeNumber> {
        if name.is_empty() || name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_ino = parent.unwrap_or(self.root_inode);
        let new_ino = self.alloc_inode();
        let mut nodes = self.nodes.write();
        let pnode = nodes.get_mut(&parent_ino).ok_or(FsError::NotFound)?;
        if pnode.node_type != KernfsNodeType::Directory {
            return Err(FsError::NotADirectory);
        }
        if pnode.children.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }
        let link_node = KernfsNode::new_symlink(new_ino, parent_ino, name, target);
        pnode.children.insert(name.to_string(), new_ino);
        pnode.metadata.modified = get_current_time();
        nodes.insert(new_ino, link_node);
        Ok(new_ino)
    }

    /// Recursively remove a node at `path`.
    pub fn remove_ns(&self, path: &str) -> FsResult<()> {
        let target_ino = self.resolve_path(path)?;
        if target_ino == self.root_inode {
            return Err(FsError::PermissionDenied);
        }
        // Collect all descendants to remove recursively.
        let to_remove: Vec<InodeNumber> = {
            let nodes = self.nodes.read();
            let mut stack = vec![target_ino];
            let mut result = Vec::new();
            while let Some(ino) = stack.pop() {
                result.push(ino);
                if let Some(node) = nodes.get(&ino) {
                    if node.node_type == KernfsNodeType::Directory {
                        for (_, &child) in &node.children {
                            if child != ino && child != node.parent {
                                stack.push(child);
                            }
                        }
                    }
                }
            }
            result
        };

        let (parent_comps, name) = Self::resolve_parent(path)?;
        let parent_ino = if parent_comps.is_empty() {
            self.root_inode
        } else {
            let nodes = self.nodes.read();
            let mut cur = self.root_inode;
            for comp in &parent_comps {
                let node = nodes.get(&cur).ok_or(FsError::NotFound)?;
                if node.node_type != KernfsNodeType::Directory {
                    return Err(FsError::NotADirectory);
                }
                cur = *node.children.get(comp).ok_or(FsError::NotFound)?;
            }
            cur
        };

        let mut nodes = self.nodes.write();
        // Check if target is a directory (for link count adjustment).
        let is_dir = nodes
            .get(&target_ino)
            .map(|n| n.node_type == KernfsNodeType::Directory)
            .unwrap_or(false);
        // Remove from parent's children.
        if let Some(pnode) = nodes.get_mut(&parent_ino) {
            pnode.children.remove(&name);
            pnode.metadata.modified = get_current_time();
            if is_dir {
                pnode.metadata.link_count = pnode.metadata.link_count.saturating_sub(1);
            }
        }
        // Remove all descendants.
        for ino in to_remove {
            nodes.remove(&ino);
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn is_dir_empty(nodes: &BTreeMap<InodeNumber, KernfsNode>, ino: InodeNumber) -> bool {
        if let Some(node) = nodes.get(&ino) {
            // Only "." and ".." entries
            node.children.len() <= 2
        } else {
            false
        }
    }
}

impl FileSystem for KernfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::SysFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let nodes = self.nodes.read();
        let count = nodes.len() as u64;
        Ok(FileSystemStats {
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: count,
            free_inodes: 0,
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_comps, filename) = Self::resolve_parent(path)?;
        if filename.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_ino = if parent_comps.is_empty() {
            self.root_inode
        } else {
            let parent_path = format!("/{}", parent_comps.join("/"));
            self.resolve_path(&parent_path)?
        };
        let new_ino = self.alloc_inode();
        let mut nodes = self.nodes.write();
        let pnode = nodes.get_mut(&parent_ino).ok_or(FsError::NotFound)?;
        if pnode.node_type != KernfsNodeType::Directory {
            return Err(FsError::NotADirectory);
        }
        if pnode.children.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }
        let mut file_node = KernfsNode::new_file(new_ino, parent_ino, &filename, permissions);
        file_node.metadata.permissions = permissions;
        pnode.children.insert(filename.clone(), new_ino);
        pnode.metadata.modified = get_current_time();
        nodes.insert(new_ino, file_node);
        Ok(new_ino)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, ino: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        // Snapshot the callback + content under the lock, then invoke the
        // callback outside the lock to avoid deadlock.
        let (read_cb, content, file_type) = {
            let nodes = self.nodes.read();
            let node = nodes.get(&ino).ok_or(FsError::NotFound)?;
            if node.node_type == KernfsNodeType::Directory {
                return Err(FsError::IsADirectory);
            }
            (node.read_cb.clone(), node.content.clone(), node.metadata.file_type)
        };
        let _ = file_type;

        let data = if let Some(cb) = read_cb {
            cb(&content, offset, buffer.len())
        } else {
            content
        };

        let data_len = data.len() as u64;
        if offset >= data_len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = core::cmp::min(start + buffer.len(), data.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&data[start..end]);

        // Update access time.
        {
            let mut nodes = self.nodes.write();
            if let Some(node) = nodes.get_mut(&ino) {
                node.metadata.accessed = get_current_time();
            }
        }
        Ok(n)
    }

    fn write(&self, ino: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        // Snapshot callback under lock, then invoke outside lock.
        let write_cb = {
            let nodes = self.nodes.read();
            let node = nodes.get(&ino).ok_or(FsError::NotFound)?;
            if node.node_type == KernfsNodeType::Directory {
                return Err(FsError::IsADirectory);
            }
            node.write_cb.clone()
        };

        let consumed = if let Some(cb) = write_cb {
            // Acquire content mutably, invoke callback outside the tree lock.
            let mut content = {
                let nodes = self.nodes.read();
                nodes.get(&ino).map(|n| n.content.clone()).unwrap_or_default()
            };
            let n = cb(&mut content, offset, buffer);
            // Persist the updated content.
            {
                let mut nodes = self.nodes.write();
                if let Some(node) = nodes.get_mut(&ino) {
                    node.content = content;
                    node.metadata.size = node.content.len() as u64;
                    node.metadata.modified = get_current_time();
                    node.metadata.accessed = node.metadata.modified;
                }
            }
            n
        } else {
            // No callback — persist bytes directly.
            let mut nodes = self.nodes.write();
            let node = nodes.get_mut(&ino).ok_or(FsError::NotFound)?;
            let required = (offset as usize).saturating_add(buffer.len());
            if node.content.len() < required {
                node.content.resize(required, 0);
            }
            node.content[offset as usize..offset as usize + buffer.len()].copy_from_slice(buffer);
            node.metadata.size = node.content.len() as u64;
            node.metadata.modified = get_current_time();
            node.metadata.accessed = node.metadata.modified;
            buffer.len()
        };
        Ok(consumed)
    }

    fn metadata(&self, ino: InodeNumber) -> FsResult<FileMetadata> {
        let nodes = self.nodes.read();
        let node = nodes.get(&ino).ok_or(FsError::NotFound)?;
        Ok(node.metadata.clone())
    }

    fn set_metadata(&self, ino: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut nodes = self.nodes.write();
        let node = nodes.get_mut(&ino).ok_or(FsError::NotFound)?;
        node.metadata.permissions = metadata.permissions;
        node.metadata.uid = metadata.uid;
        node.metadata.gid = metadata.gid;
        node.metadata.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_comps, dirname) = Self::resolve_parent(path)?;
        if dirname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_ino = if parent_comps.is_empty() {
            self.root_inode
        } else {
            let parent_path = format!("/{}", parent_comps.join("/"));
            self.resolve_path(&parent_path)?
        };
        let new_ino = self.alloc_inode();
        let mut nodes = self.nodes.write();
        let pnode = nodes.get_mut(&parent_ino).ok_or(FsError::NotFound)?;
        if pnode.node_type != KernfsNodeType::Directory {
            return Err(FsError::NotADirectory);
        }
        if pnode.children.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }
        let mut dir_node = KernfsNode::new_dir(new_ino, parent_ino, &dirname, permissions);
        dir_node.metadata.permissions = permissions;
        dir_node.children.insert(".".to_string(), new_ino);
        dir_node.children.insert("..".to_string(), parent_ino);
        pnode.children.insert(dirname.clone(), new_ino);
        pnode.metadata.modified = get_current_time();
        pnode.metadata.link_count = pnode.metadata.link_count.saturating_add(1);
        nodes.insert(new_ino, dir_node);
        Ok(new_ino)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        if path == "/" {
            return Err(FsError::PermissionDenied);
        }
        let target_ino = self.resolve_path(path)?;
        let (parent_comps, dirname) = Self::resolve_parent(path)?;
        let parent_ino = if parent_comps.is_empty() {
            self.root_inode
        } else {
            let parent_path = format!("/{}", parent_comps.join("/"));
            self.resolve_path(&parent_path)?
        };

        let mut nodes = self.nodes.write();
        let node = nodes.get(&target_ino).ok_or(FsError::NotFound)?;
        if node.node_type != KernfsNodeType::Directory {
            return Err(FsError::NotADirectory);
        }
        if !Self::is_dir_empty(&nodes, target_ino) {
            return Err(FsError::DirectoryNotEmpty);
        }
        // Remove from parent.
        if let Some(pnode) = nodes.get_mut(&parent_ino) {
            pnode.children.remove(&dirname);
            pnode.metadata.modified = get_current_time();
            pnode.metadata.link_count = pnode.metadata.link_count.saturating_sub(1);
        }
        nodes.remove(&target_ino);
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let target_ino = self.resolve_path(path)?;
        let (parent_comps, filename) = Self::resolve_parent(path)?;
        let parent_ino = if parent_comps.is_empty() {
            self.root_inode
        } else {
            let parent_path = format!("/{}", parent_comps.join("/"));
            self.resolve_path(&parent_path)?
        };
        let mut nodes = self.nodes.write();
        let node = nodes.get(&target_ino).ok_or(FsError::NotFound)?;
        if node.node_type == KernfsNodeType::Directory {
            return Err(FsError::IsADirectory);
        }
        if let Some(pnode) = nodes.get_mut(&parent_ino) {
            pnode.children.remove(&filename);
            pnode.metadata.modified = get_current_time();
        }
        nodes.remove(&target_ino);
        Ok(())
    }

    fn readdir(&self, ino: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        // Snapshot children list under lock, then look up types.
        let children: Vec<(String, InodeNumber)> = {
            let nodes = self.nodes.read();
            let node = nodes.get(&ino).ok_or(FsError::NotFound)?;
            if node.node_type != KernfsNodeType::Directory {
                return Err(FsError::NotADirectory);
            }
            node.children.iter().map(|(k, &v)| (k.clone(), v)).collect()
        };
        let nodes = self.nodes.read();
        let mut entries = Vec::new();
        for (name, child_ino) in children {
            let ft = nodes.get(&child_ino).map(|n| n.metadata.file_type).unwrap_or(FileType::Regular);
            entries.push(DirectoryEntry {
                name,
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
        let (old_parent_comps, old_name) = Self::resolve_parent(old_path)?;
        let (new_parent_comps, new_name) = Self::resolve_parent(new_path)?;
        if new_name.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let old_parent_ino = if old_parent_comps.is_empty() {
            self.root_inode
        } else {
            let p = format!("/{}", old_parent_comps.join("/"));
            self.resolve_path(&p)?
        };
        let new_parent_ino = if new_parent_comps.is_empty() {
            self.root_inode
        } else {
            let p = format!("/{}", new_parent_comps.join("/"));
            self.resolve_path(&p)?
        };

        let mut nodes = self.nodes.write();
        // Check destination doesn't exist.
        if let Some(np) = nodes.get(&new_parent_ino) {
            if np.children.contains_key(&new_name) {
                return Err(FsError::AlreadyExists);
            }
        }
        // Remove from old parent.
        if let Some(op) = nodes.get_mut(&old_parent_ino) {
            op.children.remove(&old_name);
            op.metadata.modified = get_current_time();
        }
        // Add to new parent.
        if let Some(np) = nodes.get_mut(&new_parent_ino) {
            np.children.insert(new_name.clone(), old_ino);
            np.metadata.modified = get_current_time();
        }
        // Update node's parent and name.
        if let Some(node) = nodes.get_mut(&old_ino) {
            node.parent = new_parent_ino;
            node.name = new_name;
            // If it's a directory, update ".." to point to new parent.
            if node.node_type == KernfsNodeType::Directory {
                node.children.insert("..".to_string(), new_parent_ino);
                if old_parent_ino != new_parent_ino {
                    if let Some(op) = nodes.get_mut(&old_parent_ino) {
                        op.metadata.link_count = op.metadata.link_count.saturating_sub(1);
                    }
                    if let Some(np) = nodes.get_mut(&new_parent_ino) {
                        np.metadata.link_count = np.metadata.link_count.saturating_add(1);
                    }
                }
            }
        }
        Ok(())
    }

    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()> {
        let (parent_comps, linkname) = Self::resolve_parent(link_path)?;
        if linkname.len() > 255 {
            return Err(FsError::NameTooLong);
        }
        let parent_ino = if parent_comps.is_empty() {
            self.root_inode
        } else {
            let p = format!("/{}", parent_comps.join("/"));
            self.resolve_path(&p)?
        };
        let new_ino = self.alloc_inode();
        let mut nodes = self.nodes.write();
        let pnode = nodes.get_mut(&parent_ino).ok_or(FsError::NotFound)?;
        if pnode.node_type != KernfsNodeType::Directory {
            return Err(FsError::NotADirectory);
        }
        if pnode.children.contains_key(&linkname) {
            return Err(FsError::AlreadyExists);
        }
        let link_node = KernfsNode::new_symlink(new_ino, parent_ino, &linkname, target);
        pnode.children.insert(linkname.clone(), new_ino);
        pnode.metadata.modified = get_current_time();
        nodes.insert(new_ino, link_node);
        Ok(())
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let ino = self.resolve_path(path)?;
        let nodes = self.nodes.read();
        let node = nodes.get(&ino).ok_or(FsError::NotFound)?;
        if node.node_type != KernfsNodeType::Symlink {
            return Err(FsError::InvalidArgument);
        }
        node.symlink_target.clone().ok_or(FsError::IoError)
    }

    fn sync(&self) -> FsResult<()> {
        // In-memory filesystem — nothing to sync.
        Ok(())
    }
}
