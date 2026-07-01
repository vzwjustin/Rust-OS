//! Ext2-style in-memory filesystem
//!
//! This is an in-memory VFS implementation that mirrors the ext2 data model:
//! inodes carry either file data (`Vec<u8>`) or directory entries
//! (`BTreeMap<String, InodeNumber>`). Path resolution walks the directory
//! tree starting from the root inode (1).
//!
//! A real ext2 implementation would additionally read/write the on-disk
//! superblock, block group descriptors, inode table, and data blocks through
//! a block-device I/O layer. The in-memory data path below is the structure
//! that the block I/O layer would populate/flush; the only thing it would add
//! is persistence and block-allocation accounting.

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

/// Maximum number of inodes the in-memory ext2 instance will hold.
const MAX_INODES: u64 = 65536;
/// Block size used for statfs accounting (matches ext2 default of 4096).
const BLOCK_SIZE: u32 = 4096;

/// In-memory ext2 filesystem
#[derive(Debug)]
pub struct Ext2FileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, Ext2Inode>>,
    next_inode: RwLock<InodeNumber>,
}

/// In-memory ext2 inode
#[derive(Debug, Clone)]
struct Ext2Inode {
    inode: InodeNumber,
    is_dir: bool,
    size: u64,
    permissions: FilePermissions,
    /// File content for regular files; empty for directories.
    data: Vec<u8>,
    /// Directory entries mapping name -> child inode number; empty for files.
    entries: BTreeMap<String, InodeNumber>,
    /// Creation time (Unix timestamp).
    created: u64,
    /// Last modification time.
    modified: u64,
    /// Last access time.
    accessed: u64,
    /// Hard link count.
    link_count: u32,
    /// Owner user ID.
    uid: u32,
    /// Owner group ID.
    gid: u32,
}

impl Ext2Inode {
    fn new_file(inode: InodeNumber, permissions: FilePermissions) -> Self {
        let now = get_current_time();
        Self {
            inode,
            is_dir: false,
            size: 0,
            permissions,
            data: Vec::new(),
            entries: BTreeMap::new(),
            created: now,
            modified: now,
            accessed: now,
            link_count: 1,
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
            data: Vec::new(),
            entries,
            created: now,
            modified: now,
            accessed: now,
            link_count: 2,
            uid: 0,
            gid: 0,
        }
    }
}

impl Ext2FileSystem {
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        // Root directory is inode 1, matching ext2 convention.
        let mut root = Ext2Inode::new_directory(1, FilePermissions::default_directory());
        root.entries.insert("..".to_string(), 1);
        inodes.insert(1, root);
        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
        })
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

    /// Resolve a full path to its inode number by walking directory entries.
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

    /// Resolve the parent directory inode and the final path component name.
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

    fn get_node(&self, inode: InodeNumber) -> FsResult<Ext2Inode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }

    /// True if the directory contains only `.` and `..`.
    fn is_directory_empty(inode: &Ext2Inode) -> bool {
        inode.entries.len() <= 2
    }
}

impl FileSystem for Ext2FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Ext2
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let used_inodes = inodes.len() as u64;

        // Account for blocks consumed by file data, rounded up to block size.
        let used_blocks: u64 = inodes
            .values()
            .map(|n| {
                let len = n.data.len() as u64;
                (len + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64
            })
            .sum();
        let total_blocks = MAX_INODES; // virtual capacity in blocks
        let free_blocks = total_blocks.saturating_sub(used_blocks);

        Ok(FileSystemStats {
            total_blocks,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: MAX_INODES,
            free_inodes: MAX_INODES.saturating_sub(used_inodes),
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
        if inodes.len() >= MAX_INODES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if !parent.is_dir {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&filename) {
            return Err(FsError::AlreadyExists);
        }

        let new_inode = self.allocate_inode();
        let file_node = Ext2Inode::new_file(new_inode, permissions);
        parent.entries.insert(filename, new_inode);
        parent.modified = get_current_time();

        inodes.insert(new_inode, file_node);
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

        let len = node.data.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = cmp::min(start + buffer.len(), node.data.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&node.data[start..end]);
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
        let required = new_size as usize;
        if node.data.len() < required {
            node.data.resize(required, 0);
        }
        let start = offset as usize;
        let end = start + buffer.len();
        node.data[start..end].copy_from_slice(buffer);

        node.size = node.data.len() as u64;
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
            link_count: node.link_count,
            device_id: None,
        })
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.permissions = metadata.permissions;
        node.uid = metadata.uid;
        node.gid = metadata.gid;
        // Size changes for regular files are honoured; directory size stays 0.
        if !node.is_dir {
            node.size = metadata.size;
            if node.data.len() < metadata.size as usize {
                node.data.resize(metadata.size as usize, 0);
            } else if node.data.len() > metadata.size as usize {
                node.data.truncate(metadata.size as usize);
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
        if inodes.len() >= MAX_INODES as usize {
            return Err(FsError::NoSpaceLeft);
        }
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        if !parent.is_dir {
            return Err(FsError::NotADirectory);
        }
        if parent.entries.contains_key(&dirname) {
            return Err(FsError::AlreadyExists);
        }

        let new_inode = self.allocate_inode();
        let mut dir_node = Ext2Inode::new_directory(new_inode, permissions);
        dir_node.entries.insert("..".to_string(), parent_inode);

        parent.entries.insert(dirname, new_inode);
        parent.modified = get_current_time();
        parent.link_count += 1;

        inodes.insert(new_inode, dir_node);
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
        if parent.link_count > 1 {
            parent.link_count -= 1;
        }

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
        // Snapshot (name, inode) pairs so we can drop the mutable borrow and
        // re-borrow immutably to look up child file types without deadlocking
        // the non-reentrant RwLock.
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
        // Destination must not already exist.
        let new_parent_node = inodes.get(&new_parent).ok_or(FsError::NotFound)?;
        if new_parent_node.entries.contains_key(&new_name) {
            return Err(FsError::AlreadyExists);
        }
        // Both parents must be directories.
        if !new_parent_node.is_dir {
            return Err(FsError::NotADirectory);
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
        // ext2 on-disk supports symlinks, but the in-memory inode model here
        // only tracks regular files and directories. Defer until a symlink
        // field is added to Ext2Inode.
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        // In-memory filesystem has no backing store to sync. A real ext2
        // implementation would flush dirty inodes and data blocks to the
        // block device here.
        Ok(())
    }
}
