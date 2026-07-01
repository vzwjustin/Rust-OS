//! Persistent storage filesystem stub.
//!
//! Provides a stub implementation of pstore (persistent storage filesystem),
//! which exposes persistent memory regions (ramoops, NVRAM, UEFI variables,
//! SELinux policy) as a read-only pseudofilesystem for crash logs and diagnostics.
//! Real implementation would read from backend storage drivers.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::vec::Vec;

/// PStore filesystem implementation stub.
#[derive(Debug)]
pub struct PStoreFileSystem;

impl PStoreFileSystem {
    pub fn new() -> FsResult<Self> {
        Ok(PStoreFileSystem)
    }
}

impl FileSystem for PStoreFileSystem {
    fn fs_type(&self) -> FileSystemType {
        // TODO: Add PStore variant to FileSystemType enum in mod.rs
        FileSystemType::RamFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        Err(FsError::NotSupported)
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::NotSupported)
    }

    fn metadata(&self, _inode: InodeNumber) -> FsResult<FileMetadata> {
        Err(FsError::NotSupported)
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

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Err(FsError::NotSupported)
    }
}
