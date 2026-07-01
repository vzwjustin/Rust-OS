//! Iomap filesystem stub scaffold
//!
//! This is a compile-clean stub for the iomap (I/O mapping) filesystem helper layer.
//! Real implementation would provide block mapping utilities for filesystems like ext4, xfs.
//! TODO: port from linux-master fs/iomap/

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::RwLock;

/// Iomap filesystem stub
#[derive(Debug)]
pub struct IomapFileSystem {
    inodes: RwLock<BTreeMap<InodeNumber, IomapInode>>,
    next_inode: RwLock<InodeNumber>,
}

/// Iomap inode stub
#[derive(Debug, Clone)]
struct IomapInode {
    inode: InodeNumber,
    is_dir: bool,
    size: u64,
    permissions: FilePermissions,
    entries: BTreeMap<String, InodeNumber>,
}

impl IomapFileSystem {
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        inodes.insert(
            1,
            IomapInode {
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

    fn get_node(&self, inode: InodeNumber) -> FsResult<IomapInode> {
        self.inodes
            .read()
            .get(&inode)
            .cloned()
            .ok_or(FsError::NotFound)
    }
}

impl FileSystem for IomapFileSystem {
    fn fs_type(&self) -> FileSystemType {
        // TODO: port from linux-master fs/iomap/ - iomap is a helper, not a standalone fs
        FileSystemType::RamFs // placeholder
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        // TODO: port from linux-master fs/iomap/Kfile
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
        Err(FsError::NotSupported)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/iomap/buffered-io.c
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/iomap/buffered-io.c
        Err(FsError::NotSupported)
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

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        Err(FsError::NotSupported)
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
