//! DEBUGFS (Debug File System) implementation
//!
//! DEBUGFS is a pseudo-filesystem that exposes kernel debugging interfaces to userspace.
//! This in-memory implementation provides a writable tree for debug data.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::RwLock;

/// DEBUGFS inode entry
#[derive(Debug, Clone)]
struct DebugfsInode {
    inode: InodeNumber,
    is_dir: bool,
    size: u64,
    permissions: FilePermissions,
    data: Vec<u8>,
    entries: BTreeMap<String, InodeNumber>,
}

/// DEBUGFS filesystem
#[derive(Debug)]
pub struct DebugfsFileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, DebugfsInode>>,
    next_inode: RwLock<InodeNumber>,
}

impl DebugfsFileSystem {
    /// Create a new DEBUGFS filesystem with a root directory.
    /// A full implementation would register debugfs subsystem handlers
    /// and set up callbacks for dynamic file generation.
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        inodes.insert(
            1,
            DebugfsInode {
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
}

impl FileSystem for DebugfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Debugfs
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
        if let Some(parent) = inodes.get(&parent_inode) {
            if parent.entries.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        }
        inodes.insert(
            inode_num,
            DebugfsInode {
                inode: inode_num,
                is_dir: false,
                size: 0,
                permissions,
                data: Vec::new(),
                entries: BTreeMap::new(),
            },
        );
        if let Some(parent) = inodes.get_mut(&parent_inode) {
            parent.entries.insert(name, inode_num);
        }
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
        if let Some(parent) = inodes.get(&parent_inode) {
            if parent.entries.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        }
        inodes.insert(
            inode_num,
            DebugfsInode {
                inode: inode_num,
                is_dir: true,
                size: 0,
                permissions,
                data: Vec::new(),
                entries: BTreeMap::new(),
            },
        );
        if let Some(parent) = inodes.get_mut(&parent_inode) {
            parent.entries.insert(name, inode_num);
        }
        Ok(inode_num)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let (parent_inode, name) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let child_inode = {
            let parent = inodes.get(&parent_inode).ok_or(FsError::NotFound)?;
            *parent.entries.get(&name).ok_or(FsError::NotFound)?
        };
        {
            let child = inodes.get(&child_inode).ok_or(FsError::NotFound)?;
            if !child.is_dir {
                return Err(FsError::NotADirectory);
            }
            if !child.entries.is_empty() {
                return Err(FsError::DirectoryNotEmpty);
            }
        }
        inodes.remove(&child_inode);
        if let Some(parent) = inodes.get_mut(&parent_inode) {
            parent.entries.remove(&name);
        }
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let (parent_inode, name) = self.resolve_parent(path)?;
        let mut inodes = self.inodes.write();
        let child_inode = {
            let parent = inodes.get(&parent_inode).ok_or(FsError::NotFound)?;
            *parent.entries.get(&name).ok_or(FsError::NotFound)?
        };
        {
            let child = inodes.get(&child_inode).ok_or(FsError::NotFound)?;
            if child.is_dir {
                return Err(FsError::IsADirectory);
            }
        }
        inodes.remove(&child_inode);
        if let Some(parent) = inodes.get_mut(&parent_inode) {
            parent.entries.remove(&name);
        }
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
            let parent = inodes.get(&old_parent).ok_or(FsError::NotFound)?;
            *parent.entries.get(&old_name).ok_or(FsError::NotFound)?
        };
        if let Some(new_p) = inodes.get(&new_parent) {
            if new_p.entries.contains_key(&new_name) {
                return Err(FsError::AlreadyExists);
            }
        }
        if let Some(old_p) = inodes.get_mut(&old_parent) {
            old_p.entries.remove(&old_name);
        }
        if let Some(new_p) = inodes.get_mut(&new_parent) {
            new_p.entries.insert(new_name, child_inode);
        }
        Ok(())
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        Err(FsError::NotASymlink)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
