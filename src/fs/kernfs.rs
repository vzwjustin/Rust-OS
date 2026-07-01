//! Kernfs in-memory virtual filesystem
//!
//! Kernfs is the kernel-internal backing store for sysfs, debugfs, cgroup2,
//! and similar pseudo-filesystems. Each file/directory is a kernel-generated
//! attribute: reads return dynamically produced content and writes update
//! kernel state. This in-memory implementation stores the generated content
//! in `Vec<u8>` per inode and walks the directory tree from root inode 1.
//!
//! A real kernfs would invoke per-kn `seq_file` show/store ops instead of
//! reading a static `data` buffer; the data path here is the cache that those
//! ops would populate.

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

/// Maximum number of kernfs nodes.
const MAX_NODES: u64 = 16384;
/// Block size reported via statfs.
const BLOCK_SIZE: u32 = 4096;

/// In-memory kernfs filesystem
#[derive(Debug)]
pub struct KernfsFileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, KernfsInode>>,
    next_inode: RwLock<InodeNumber>,
}

/// In-memory kernfs inode (kernel node)
#[derive(Debug, Clone)]
struct KernfsInode {
    inode: InodeNumber,
    is_dir: bool,
    size: u64,
    permissions: FilePermissions,
    /// Generated file content for regular kns; empty for directories.
    data: Vec<u8>,
    /// Directory entries mapping name -> child inode number.
    entries: BTreeMap<String, InodeNumber>,
    /// Creation time (Unix timestamp).
    created: u64,
    /// Last modification time.
    modified: u64,
    /// Last access time.
    accessed: u64,
    /// Owner user ID.
    uid: u32,
    /// Owner group ID.
    gid: u32,
}

impl KernfsInode {
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
            uid: 0,
            gid: 0,
        }
    }
}

impl KernfsFileSystem {
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        let mut root = KernfsInode::new_directory(1, FilePermissions::default_directory());
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

    fn get_node(&self, inode: InodeNumber) -> FsResult<KernfsInode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }

    fn is_directory_empty(inode: &KernfsInode) -> bool {
        inode.entries.len() <= 2
    }
}

impl FileSystem for KernfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        // Kernfs is the backing layer for sysfs/debugfs/cgroup2; report SysFs
        // since there is no dedicated FileSystemType variant for kernfs.
        FileSystemType::SysFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        let used = inodes.len() as u64;
        let used_blocks: u64 = inodes
            .values()
            .map(|n| {
                let len = n.data.len() as u64;
                (len + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64
            })
            .sum();
        let free_blocks = MAX_NODES.saturating_sub(used_blocks);
        Ok(FileSystemStats {
            total_blocks: MAX_NODES,
            free_blocks,
            available_blocks: free_blocks,
            total_inodes: MAX_NODES,
            free_inodes: MAX_NODES.saturating_sub(used),
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
        if inodes.len() >= MAX_NODES as usize {
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
        let node = KernfsInode::new_file(new_inode, permissions);
        parent.entries.insert(filename, new_inode);
        parent.modified = get_current_time();
        inodes.insert(new_inode, node);
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
            link_count: 1,
            device_id: None,
        })
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.permissions = metadata.permissions;
        node.uid = metadata.uid;
        node.gid = metadata.gid;
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
        if inodes.len() >= MAX_NODES as usize {
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
        let mut dir = KernfsInode::new_directory(new_inode, permissions);
        dir.entries.insert("..".to_string(), parent_inode);
        parent.entries.insert(dirname, new_inode);
        parent.modified = get_current_time();
        inodes.insert(new_inode, dir);
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
        let new_parent_node = inodes.get(&new_parent).ok_or(FsError::NotFound)?;
        if !new_parent_node.is_dir {
            return Err(FsError::NotADirectory);
        }
        if new_parent_node.entries.contains_key(&new_name) {
            return Err(FsError::AlreadyExists);
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
        // Kernfs does not expose symlinks through this interface.
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        // Pure in-memory; nothing to flush.
        Ok(())
    }
}
