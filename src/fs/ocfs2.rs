//! OCFS2 (Oracle Cluster File System 2) in-memory implementation.
//!
//! OCFS2 is a general-purpose cluster filesystem designed for RAC (Real
//! Application Cluster) environments. This implementation provides a fully
//! functional in-memory VFS with cluster node tracking and a distributed lock
//! manager state table. On-disk journaling and heartbeat are out of scope for
//! the in-memory model; instead, cluster membership and lock acquisition are
//! tracked in memory so that the filesystem behaves correctly for single-node
//! operation and exposes the cluster state to callers.

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

/// Maximum file size in the in-memory OCFS2 filesystem (16 MiB).
const MAX_FILE_SIZE: u64 = 16 * 1024 * 1024;
/// Maximum number of inodes.
const MAX_INODES: u64 = 4096;

/// Cluster node state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    /// Node is online and participating in the cluster.
    Online,
    /// Node is offline / has left the cluster.
    Offline,
}

/// A cluster node descriptor.
#[derive(Debug, Clone)]
pub struct ClusterNode {
    /// Node id (unique within the cluster).
    pub node_id: u32,
    /// Human-readable node name.
    pub name: String,
    /// Current node state.
    pub state: NodeState,
}

/// Distributed lock manager lock mode (mirrors DLM lock modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// No lock.
    None,
    /// Shared (read) lock.
    Shared,
    /// Exclusive (write) lock.
    Exclusive,
}

/// A DLM lock entry.
#[derive(Debug, Clone)]
struct DlmLock {
    /// Resource name being locked (typically a path or inode key).
    resource: String,
    /// Node holding the lock.
    node_id: u32,
    /// Lock mode.
    mode: LockMode,
}

/// In-memory inode.
#[derive(Debug, Clone)]
struct Ocfs2Inode {
    metadata: FileMetadata,
    content: Vec<u8>,
    entries: BTreeMap<String, InodeNumber>,
    symlink_target: Option<String>,
}

impl Ocfs2Inode {
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
            content: Vec::new(),
            entries: BTreeMap::new(),
            symlink_target: Some(target.to_string()),
        }
    }
}

/// OCFS2 filesystem.
#[derive(Debug)]
pub struct Ocfs2FileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, Ocfs2Inode>>,
    next_inode: RwLock<InodeNumber>,
    root_inode: InodeNumber,
    /// Cluster membership: node id -> node descriptor.
    nodes: RwLock<BTreeMap<u32, ClusterNode>>,
    /// Active DLM locks keyed by resource name.
    locks: RwLock<BTreeMap<String, DlmLock>>,
    /// The local node id.
    local_node_id: u32,
}

impl Ocfs2FileSystem {
    /// Create a new OCFS2 filesystem instance.
    ///
    /// Initializes the cluster with a single online local node (id 0) so the
    /// filesystem is immediately usable for single-node operation.
    pub fn new() -> FsResult<Self> {
        let root_inode = 1;
        let mut root = Ocfs2Inode::new_directory(root_inode, FilePermissions::default_directory());
        root.entries.insert("..".to_string(), root_inode);
        let mut inodes = BTreeMap::new();
        inodes.insert(root_inode, root);

        let mut nodes = BTreeMap::new();
        nodes.insert(
            0,
            ClusterNode {
                node_id: 0,
                name: "local".to_string(),
                state: NodeState::Online,
            },
        );

        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
            root_inode,
            nodes: RwLock::new(nodes),
            locks: RwLock::new(BTreeMap::new()),
            local_node_id: 0,
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

    fn is_directory_empty(&self, inode: InodeNumber) -> FsResult<bool> {
        let inodes = self.inodes.read();
        let dir = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        Ok(dir.entries.len() <= 2)
    }

    // ── Cluster membership API ─────────────────────────────────────────────

    /// Add a cluster node.
    pub fn add_node(&self, node_id: u32, name: &str) -> FsResult<()> {
        let mut nodes = self.nodes.write();
        if nodes.contains_key(&node_id) {
            return Err(FsError::AlreadyExists);
        }
        nodes.insert(
            node_id,
            ClusterNode {
                node_id,
                name: name.to_string(),
                state: NodeState::Online,
            },
        );
        Ok(())
    }

    /// Mark a cluster node offline (e.g. after heartbeat loss).
    pub fn set_node_offline(&self, node_id: u32) -> FsResult<()> {
        let mut nodes = self.nodes.write();
        let node = nodes.get_mut(&node_id).ok_or(FsError::NotFound)?;
        node.state = NodeState::Offline;
        Ok(())
    }

    /// Get the list of online cluster nodes.
    pub fn online_nodes(&self) -> Vec<ClusterNode> {
        self.nodes
            .read()
            .values()
            .filter(|n| n.state == NodeState::Online)
            .cloned()
            .collect()
    }

    // ── DLM API ────────────────────────────────────────────────────────────

    /// Acquire a DLM lock on a resource.
    ///
    /// Returns `AlreadyExists` if a conflicting lock is held by another node.
    pub fn acquire_lock(&self, resource: &str, mode: LockMode) -> FsResult<()> {
        let mut locks = self.locks.write();
        if let Some(existing) = locks.get(resource) {
            if existing.node_id != self.local_node_id {
                if existing.mode == LockMode::Exclusive || mode == LockMode::Exclusive {
                    return Err(FsError::AlreadyExists);
                }
            }
        }
        locks.insert(
            resource.to_string(),
            DlmLock {
                resource: resource.to_string(),
                node_id: self.local_node_id,
                mode,
            },
        );
        Ok(())
    }

    /// Release a DLM lock held by the local node.
    pub fn release_lock(&self, resource: &str) -> FsResult<()> {
        let mut locks = self.locks.write();
        let existing = locks.get(resource).ok_or(FsError::NotFound)?;
        if existing.node_id != self.local_node_id {
            return Err(FsError::PermissionDenied);
        }
        locks.remove(resource);
        Ok(())
    }

    /// Get the current lock mode for a resource (or `None` if unlocked).
    pub fn lock_mode(&self, resource: &str) -> Option<LockMode> {
        self.locks.read().get(resource).map(|l| l.mode)
    }
}

impl FileSystem for Ocfs2FileSystem {
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
        let total_blocks = (MAX_FILE_SIZE * MAX_INODES) / block_size as u64;
        let free_blocks = total_blocks.saturating_sub(used_blocks);
        Ok(FileSystemStats {
            total_blocks,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: MAX_INODES,
            free_inodes: MAX_INODES.saturating_sub(used),
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
        if inodes.len() >= MAX_INODES as usize {
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
        let file_inode = Ocfs2Inode::new_file(new_inode, permissions);
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
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        node.metadata.accessed = get_current_time();
        let len = node.content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = core::cmp::min(start + buffer.len(), node.content.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&node.content[start..end]);
        Ok(n)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Regular {
            return Err(FsError::IsADirectory);
        }
        let new_size = offset
            .checked_add(buffer.len() as u64)
            .ok_or(FsError::InvalidArgument)?;
        if new_size > MAX_FILE_SIZE {
            return Err(FsError::NoSpaceLeft);
        }
        let required = new_size as usize;
        if node.content.len() < required {
            node.content.resize(required, 0);
        }
        let start = offset as usize;
        let end = start + buffer.len();
        node.content[start..end].copy_from_slice(buffer);
        node.metadata.size = node.content.len() as u64;
        node.metadata.modified = get_current_time();
        node.metadata.accessed = get_current_time();
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
        if inodes.len() >= MAX_INODES as usize {
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
        let mut dir = Ocfs2Inode::new_directory(new_inode, permissions);
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
        let dir = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if dir.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        dir.metadata.accessed = get_current_time();
        let snapshot: Vec<(String, InodeNumber)> =
            dir.entries.iter().map(|(n, &i)| (n.clone(), i)).collect();
        let mut out = Vec::new();
        for (name, child_inode) in snapshot {
            if let Some(child) = inodes.get(&child_inode) {
                out.push(DirectoryEntry {
                    name,
                    inode: child_inode,
                    file_type: child.metadata.file_type,
                });
            }
        }
        Ok(out)
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
        if inodes.len() >= MAX_INODES as usize {
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
        let sym = Ocfs2Inode::new_symlink(new_inode, target);
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
        // In-memory filesystem; nothing to flush.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_and_locks() {
        let fs = Ocfs2FileSystem::new().unwrap();
        assert_eq!(fs.online_nodes().len(), 1);
        fs.add_node(1, "node1").unwrap();
        assert_eq!(fs.online_nodes().len(), 2);
        fs.set_node_offline(1).unwrap();
        assert_eq!(fs.online_nodes().len(), 1);
        fs.acquire_lock("/res", LockMode::Exclusive).unwrap();
        assert_eq!(fs.lock_mode("/res"), Some(LockMode::Exclusive));
        fs.release_lock("/res").unwrap();
        assert_eq!(fs.lock_mode("/res"), None);
    }

    #[test]
    fn test_file_ops() {
        let fs = Ocfs2FileSystem::new().unwrap();
        let inode = fs
            .create("/file.txt", FilePermissions::default_file())
            .unwrap();
        let n = fs.write(inode, 0, b"hello").unwrap();
        assert_eq!(n, 5);
        let mut buf = [0u8; 5];
        let r = fs.read(inode, 0, &mut buf).unwrap();
        assert_eq!(r, 5);
        assert_eq!(&buf, b"hello");
    }
}
