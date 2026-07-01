//! GFS2 (Global File System 2) implementation
//!
//! GFS2 is a cluster filesystem used in high-availability environments.
//! This in-memory implementation provides VFS operations with cluster node
//! tracking and distributed lock manager (DLM) state.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::RwLock;

/// GFS2 inode entry
#[derive(Debug, Clone)]
struct Gfs2Inode {
    inode: InodeNumber,
    is_dir: bool,
    size: u64,
    permissions: FilePermissions,
    data: Vec<u8>,
    entries: BTreeMap<String, InodeNumber>,
}

/// Cluster node state
#[derive(Debug, Clone)]
struct ClusterNode {
    node_id: u32,
    online: bool,
}

/// GFS2 filesystem with cluster state
#[derive(Debug)]
pub struct Gfs2FileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, Gfs2Inode>>,
    next_inode: RwLock<InodeNumber>,
    nodes: RwLock<BTreeMap<u32, ClusterNode>>,
    journal_sequence: RwLock<u64>,
}

impl Gfs2FileSystem {
    /// Create a new GFS2 filesystem. A full implementation would parse
    /// cluster membership metadata, initialize cluster locking, and set
    /// up journal recovery.
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        inodes.insert(
            1,
            Gfs2Inode {
                inode: 1,
                is_dir: true,
                size: 0,
                permissions: FilePermissions::default_directory(),
                data: Vec::new(),
                entries: BTreeMap::new(),
            },
        );
        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
            nodes: RwLock::new(BTreeMap::new()),
            journal_sequence: RwLock::new(0),
        })
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            return Ok(1);
        }
        let inodes = self.inodes.read();
        let mut current = 1u64;
        for component in path.split('/') {
            if component.is_empty() {
                continue;
            }
            let node = inodes.get(&current).ok_or(FsError::NotFound)?;
            if !node.is_dir {
                return Err(FsError::NotADirectory);
            }
            current = *node.entries.get(component).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }

    fn resolve_parent(&self, path: &str) -> FsResult<(InodeNumber, String)> {
        let (parent_path, name) = match path.rfind('/') {
            Some(idx) => (&path[..idx], &path[idx + 1..]),
            None => ("", path),
        };
        if name.is_empty() {
            return Err(FsError::InvalidPath);
        }
        Ok((self.resolve_path(parent_path)?, String::from(name)))
    }

    fn allocate_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let id = *next;
        *next += 1;
        id
    }

    /// Add a cluster node.
    pub fn add_node(&self, node_id: u32) {
        self.nodes.write().insert(
            node_id,
            ClusterNode {
                node_id,
                online: true,
            },
        );
    }

    /// Mark a cluster node as offline.
    pub fn set_node_offline(&self, node_id: u32) {
        if let Some(node) = self.nodes.write().get_mut(&node_id) {
            node.online = false;
        }
    }

    /// Get the number of online cluster nodes.
    pub fn online_node_count(&self) -> usize {
        self.nodes.read().values().filter(|n| n.online).count()
    }

    /// Get the current journal sequence number.
    pub fn journal_seq(&self) -> u64 {
        *self.journal_sequence.read()
    }
}

impl FileSystem for Gfs2FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Gfs2
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

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, name) = self.resolve_parent(path)?;
        let inode_num = self.allocate_inode();
        let mut inodes = self.inodes.write();
        if let Some(p) = inodes.get(&parent_inode) {
            if p.entries.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        }
        inodes.insert(
            inode_num,
            Gfs2Inode {
                inode: inode_num,
                is_dir: false,
                size: 0,
                permissions,
                data: Vec::new(),
                entries: BTreeMap::new(),
            },
        );
        if let Some(p) = inodes.get_mut(&parent_inode) {
            p.entries.insert(name, inode_num);
        }
        *self.journal_sequence.write() += 1;
        Ok(inode_num)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        let offset = offset as usize;
        if offset >= node.data.len() {
            return Ok(0);
        }
        let to_read = buffer.len().min(node.data.len() - offset);
        buffer[..to_read].copy_from_slice(&node.data[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        if node.is_dir {
            return Err(FsError::IsADirectory);
        }
        let offset = offset as usize;
        let end = offset + buffer.len();
        if end > node.data.len() {
            node.data.resize(end, 0);
        }
        node.data[offset..end].copy_from_slice(buffer);
        node.size = node.data.len() as u64;
        *self.journal_sequence.write() += 1;
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        let now = get_current_time();
        Ok(FileMetadata {
            inode,
            file_type: if node.is_dir {
                FileType::Directory
            } else {
                FileType::Regular
            },
            size: node.size,
            permissions: node.permissions,
            uid: 0,
            gid: 0,
            created: now,
            modified: now,
            accessed: now,
            link_count: 1,
            device_id: None,
        })
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.permissions = metadata.permissions;
        node.size = metadata.size;
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_inode, name) = self.resolve_parent(path)?;
        let inode_num = self.allocate_inode();
        let mut inodes = self.inodes.write();
        if let Some(p) = inodes.get(&parent_inode) {
            if p.entries.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        }
        inodes.insert(
            inode_num,
            Gfs2Inode {
                inode: inode_num,
                is_dir: true,
                size: 0,
                permissions,
                data: Vec::new(),
                entries: BTreeMap::new(),
            },
        );
        if let Some(p) = inodes.get_mut(&parent_inode) {
            p.entries.insert(name, inode_num);
        }
        *self.journal_sequence.write() += 1;
        Ok(inode_num)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let (parent_inode, name) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let child_inode = {
            let p = inodes.get(&parent_inode).ok_or(FsError::NotFound)?;
            *p.entries.get(&name).ok_or(FsError::NotFound)?
        };
        {
            let c = inodes.get(&child_inode).ok_or(FsError::NotFound)?;
            if !c.is_dir {
                return Err(FsError::NotADirectory);
            }
            if !c.entries.is_empty() {
                return Err(FsError::DirectoryNotEmpty);
            }
        }
        inodes.remove(&child_inode);
        if let Some(p) = inodes.get_mut(&parent_inode) {
            p.entries.remove(&name);
        }
        *self.journal_sequence.write() += 1;
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let (parent_inode, name) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let child_inode = {
            let p = inodes.get(&parent_inode).ok_or(FsError::NotFound)?;
            *p.entries.get(&name).ok_or(FsError::NotFound)?
        };
        {
            let c = inodes.get(&child_inode).ok_or(FsError::NotFound)?;
            if c.is_dir {
                return Err(FsError::IsADirectory);
            }
        }
        inodes.remove(&child_inode);
        if let Some(p) = inodes.get_mut(&parent_inode) {
            p.entries.remove(&name);
        }
        *self.journal_sequence.write() += 1;
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }
        let mut entries = Vec::new();
        for (name, &child_inode) in &node.entries {
            if let Some(child) = inodes.get(&child_inode) {
                entries.push(DirectoryEntry {
                    name: name.clone(),
                    inode: child_inode,
                    file_type: if child.is_dir {
                        FileType::Directory
                    } else {
                        FileType::Regular
                    },
                });
            }
        }
        Ok(entries)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let (old_parent, old_name) = self.resolve_parent(old_path)?;
        let (new_parent, new_name) = self.resolve_parent(new_path)?;
        let mut inodes = self.inodes.write();
        let child_inode = {
            let p = inodes.get(&old_parent).ok_or(FsError::NotFound)?;
            *p.entries.get(&old_name).ok_or(FsError::NotFound)?
        };
        if let Some(np) = inodes.get(&new_parent) {
            if np.entries.contains_key(&new_name) {
                return Err(FsError::AlreadyExists);
            }
        }
        if let Some(op) = inodes.get_mut(&old_parent) {
            op.entries.remove(&old_name);
        }
        if let Some(np) = inodes.get_mut(&new_parent) {
            np.entries.insert(new_name, child_inode);
        }
        *self.journal_sequence.write() += 1;
        Ok(())
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        Err(FsError::NotASymlink)
    }

    fn sync(&self) -> FsResult<()> {
        *self.journal_sequence.write() += 1;
        Ok(())
    }
}
