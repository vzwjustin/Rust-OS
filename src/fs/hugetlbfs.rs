//! HugetlbFS filesystem for huge page memory pools
//!
//! A simple stub for the Linux hugetlbfs virtual filesystem that provides
//! access to huge memory pages (2MiB, 1GiB) for performance-critical applications.
//! Full implementation requires port from linux-master fs/hugetlbfs.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::vec::Vec;
use core::fmt;

/// HugetlbFS filesystem
#[derive(Debug)]
pub struct HugetlbFs;

impl fmt::Display for HugetlbFs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "hugetlbfs")
    }
}

impl FileSystem for HugetlbFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::HugetlbFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_statfs)
        Err(FsError::NotSupported)
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_create)
        Err(FsError::NotSupported)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_open)
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_read)
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_write)
        Err(FsError::NotSupported)
    }

    fn metadata(&self, _inode: InodeNumber) -> FsResult<FileMetadata> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_getattr)
        Err(FsError::NotSupported)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_setattr)
        Err(FsError::NotSupported)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_mkdir)
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_rmdir)
        Err(FsError::NotSupported)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_unlink)
        Err(FsError::NotSupported)
    }

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_readdir)
        Err(FsError::NotSupported)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_rename)
        Err(FsError::NotSupported)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_symlink)
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        // TODO: port from linux-master fs/hugetlbfs/inode.c (hugetlbfs_readlink)
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
