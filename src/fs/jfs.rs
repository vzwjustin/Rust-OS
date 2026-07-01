//! JFS (Journaled File System) stub implementation
//!
//! JFS is a high-performance journaled filesystem with extent-based allocation.
//! This is a compile-clean stub; full implementation would require parsing on-disk
//! structures, managing the journal, and implementing extent-based allocation.
//! See: linux-master/fs/jfs/

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::vec::Vec;

/// JFS filesystem (stub)
#[derive(Debug)]
pub struct JfsFileSystem;

impl JfsFileSystem {
    /// Create a new JFS filesystem instance
    pub fn new() -> FsResult<Self> {
        // TODO: port from linux-master fs/jfs/
        // - Parse superblock and aggregate inode maps
        // - Initialize journal recovery mechanism
        // - Set up extent allocation tree (B+ tree)
        Ok(Self)
    }
}

impl FileSystem for JfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        // JFS not yet in FileSystemType enum; would add it
        FileSystemType::RamFs // Stub
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
