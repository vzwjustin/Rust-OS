//! Automount filesystem implementation
//!
//! Provides an interface between the kernel and an automount daemon for managing
//! mount points dynamically. This in-memory implementation tracks mount entries
//! and automount triggers.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::RwLock;

/// Automount entry state
#[derive(Debug, Clone)]
struct AutoFsEntry {
    inode: InodeNumber,
    name: String,
    is_dir: bool,
    is_mounted: bool,
    mount_target: Option<String>,
    entries: BTreeMap<String, InodeNumber>,
}

/// AutoFS filesystem
#[derive(Debug)]
pub struct AutoFsFileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, AutoFsEntry>>,
    next_inode: RwLock<InodeNumber>,
}

impl AutoFsFileSystem {
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        inodes.insert(1, AutoFsEntry {
            inode: 1, name: String::from("/"), is_dir: true,
            is_mounted: false, mount_target: None, entries: BTreeMap::new(),
        });
        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
        })
    }

    fn resolve_path(&self, path: &str) -> FsResult<InodeNumber> {
        let path = path.trim_start_matches('/');
        if path.is_empty() { return Ok(1); }
        let inodes = self.inodes.read();
        let mut current = 1u64;
        for component in path.split('/') {
            if component.is_empty() { continue; }
            let node = inodes.get(&current).ok_or(FsError::NotFound)?;
            if !node.is_dir { return Err(FsError::NotADirectory); }
            current = *node.entries.get(component).ok_or(FsError::NotFound)?;
        }
        Ok(current)
    }

    /// Add an automount trigger entry. When accessed, the automount daemon
    /// would mount the target at this path.
    pub fn add_automount(&self, path: &str, target: &str) -> FsResult<InodeNumber> {
        let (parent_path, name) = match path.rfind('/') {
            Some(idx) => (&path[..idx], &path[idx + 1..]),
            None => ("", path),
        };
        let parent_inode = self.resolve_path(parent_path)?;
        let mut next = self.next_inode.write();
        let inode_num = *next;
        *next += 1;
        let mut inodes = self.inodes.write();
        inodes.insert(inode_num, AutoFsEntry {
            inode: inode_num, name: String::from(name), is_dir: true,
            is_mounted: false, mount_target: Some(String::from(target)),
            entries: BTreeMap::new(),
        });
        if let Some(p) = inodes.get_mut(&parent_inode) {
            p.entries.insert(String::from(name), inode_num);
        }
        Ok(inode_num)
    }

    /// Mark a mount point as mounted (automount daemon completed).
    pub fn set_mounted(&self, path: &str, mounted: bool) -> FsResult<()> {
        let inode = self.resolve_path(path)?;
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.is_mounted = mounted;
        Ok(())
    }

    /// Check if a path is currently mounted.
    pub fn is_mounted(&self, path: &str) -> FsResult<bool> {
        let inode = self.resolve_path(path)?;
        let inodes = self.inodes.read();
        Ok(inodes.get(&inode).ok_or(FsError::NotFound)?.is_mounted)
    }
}

impl FileSystem for AutoFsFileSystem {
    fn fs_type(&self) -> FileSystemType { FileSystemType::AutoFs }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let inodes = self.inodes.read();
        Ok(FileSystemStats {
            total_blocks: 0, free_blocks: 0, available_blocks: 0,
            total_inodes: inodes.len() as u64, free_inodes: 0,
            block_size: 4096, max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve_path(path)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::NotSupported)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        Ok(FileMetadata {
            inode, file_type: FileType::Directory,
            size: 0, permissions: FilePermissions::default_directory(),
            uid: 0, gid: 0, created: 0, modified: 0, accessed: 0,
            link_count: 1, device_id: None,
        })
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if !node.is_dir { return Err(FsError::NotADirectory); }
        let mut entries = Vec::new();
        for (name, &child_inode) in &node.entries {
            entries.push(DirectoryEntry {
                name: name.clone(), inode: child_inode, file_type: FileType::Directory,
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

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        Err(FsError::NotASymlink)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
