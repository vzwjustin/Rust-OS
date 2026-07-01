//! Procfs filesystem stub scaffold
//!
//! This is a compile-clean stub for the proc filesystem.
//! Real implementation would dynamically generate per-PID directories backed by process table.
//! Files like /proc/self/maps, /proc/[pid]/status provide process introspection.
//! TODO: port from linux-master fs/proc/

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::RwLock;

/// Procfs filesystem stub
#[derive(Debug)]
pub struct ProcFileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, ProcInode>>,
    next_inode: RwLock<InodeNumber>,
}

/// Procfs inode stub
#[derive(Debug, Clone)]
struct ProcInode {
    inode: InodeNumber,
    is_dir: bool,
    size: u64,
    permissions: FilePermissions,
    entries: BTreeMap<String, InodeNumber>,
}

impl ProcFileSystem {
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        inodes.insert(
            1,
            ProcInode {
                inode: 1,
                is_dir: true,
                size: 0,
                permissions: FilePermissions::default_directory(),
                entries: BTreeMap::new(),
            },
        );
        Ok(Self {
            inodes: RwLock::new(inodes),
            next_inode: RwLock::new(2),
        })
    }

    fn get_node(&self, inode: InodeNumber) -> FsResult<ProcInode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }
}

impl FileSystem for ProcFileSystem {
    fn fs_type(&self) -> FileSystemType {
        // TODO: port from linux-master fs/proc/ - add FileSystemType::Proc
        FileSystemType::RamFs // placeholder
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        // TODO: port from linux-master fs/proc/inode.c
        Ok(FileSystemStats {
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: 0,
            free_inodes: 0,
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // Procfs is read-only for most entries
        Err(FsError::ReadOnly)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/proc/namei.c proc_lookup
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/proc/base.c proc_pid_read
        // This would query the process table and generate content on-the-fly
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // Procfs is read-only
        Err(FsError::ReadOnly)
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
            uid: 0,
            gid: 0,
            created: 0,
            modified: 0,
            accessed: 0,
            link_count: 1,
            device_id: None,
        })
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // Procfs is read-only
        Err(FsError::ReadOnly)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // Procfs is read-only
        Err(FsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        // Procfs is read-only
        Err(FsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        // Procfs is read-only
        Err(FsError::ReadOnly)
    }

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        // TODO: port from linux-master fs/proc/namei.c proc_readdir
        // This would query the process table and generate directory entries on-the-fly
        Err(FsError::NotSupported)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // Procfs is read-only
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        // Procfs is virtual, no syncing needed
        Ok(())
    }
}
