//! DevPTS pseudo-filesystem for pseudo-terminal slaves
//!
//! A simple stub for the Linux devpts virtual filesystem that provides
//! access to pseudo-terminal device files. Full implementation requires
//! port from linux-master fs/devpts.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::vec::Vec;
use core::fmt;

/// DevPTS filesystem
#[derive(Debug)]
pub struct DevPtsFs;

impl fmt::Display for DevPtsFs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "devpts")
    }
}

impl FileSystem for DevPtsFs {
    fn fs_type(&self) -> FileSystemType {
        // TODO: add DevPts variant to FileSystemType enum
        // For now, return NotSupported since FileSystemType doesn't have DevPts yet
        FileSystemType::DevFs // placeholder
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        // TODO: port from linux-master fs/devpts/inode.c
        Err(FsError::NotSupported)
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_create)
        Err(FsError::NotSupported)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_open)
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_read)
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_write)
        Err(FsError::NotSupported)
    }

    fn metadata(&self, _inode: InodeNumber) -> FsResult<FileMetadata> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_getattr)
        Err(FsError::NotSupported)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_setattr)
        Err(FsError::NotSupported)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_mkdir)
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_rmdir)
        Err(FsError::NotSupported)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_unlink)
        Err(FsError::NotSupported)
    }

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_readdir)
        Err(FsError::NotSupported)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_rename)
        Err(FsError::NotSupported)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_symlink)
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        // TODO: port from linux-master fs/devpts/inode.c (devpts_readlink)
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
