//! ConfigFS pseudo-filesystem for kernel configuration
//!
//! A simple stub for the Linux configfs virtual filesystem that allows
//! users and applications to create/manage kernel objects through filesystem
//! operations. Full implementation requires port from linux-master fs/configfs.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::vec::Vec;
use core::fmt;

/// ConfigFS filesystem
#[derive(Debug)]
pub struct ConfigFs;

impl fmt::Display for ConfigFs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "configfs")
    }
}

impl FileSystem for ConfigFs {
    fn fs_type(&self) -> FileSystemType {
        // TODO: add ConfigFs variant to FileSystemType enum
        // For now, return NotSupported since FileSystemType doesn't have ConfigFs yet
        FileSystemType::RamFs // placeholder
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        // TODO: port from linux-master fs/configfs/configfs_internal.h
        Err(FsError::NotSupported)
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_create)
        Err(FsError::NotSupported)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_open)
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_read)
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_write)
        Err(FsError::NotSupported)
    }

    fn metadata(&self, _inode: InodeNumber) -> FsResult<FileMetadata> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_getattr)
        Err(FsError::NotSupported)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_setattr)
        Err(FsError::NotSupported)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_mkdir)
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_rmdir)
        Err(FsError::NotSupported)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_unlink)
        Err(FsError::NotSupported)
    }

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_readdir)
        Err(FsError::NotSupported)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_rename)
        Err(FsError::NotSupported)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_symlink)
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        // TODO: port from linux-master fs/configfs/configfs.c (configfs_readlink)
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
