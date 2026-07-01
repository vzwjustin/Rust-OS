//! GFS2 (Global File System 2) stub implementation
//!
//! GFS2 is a cluster filesystem used in high-availability environments.
//! This is a compile-clean stub; full implementation would require cluster-aware
//! locking, journal recovery, and network communication with cluster nodes.
//! See: linux-master/fs/gfs2/

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::vec::Vec;

/// GFS2 filesystem (stub)
#[derive(Debug)]
pub struct Gfs2FileSystem;

impl Gfs2FileSystem {
    /// Create a new GFS2 filesystem instance
    pub fn new() -> FsResult<Self> {
        // TODO: port from linux-master fs/gfs2/
        // - Parse cluster membership metadata
        // - Initialize cluster locking mechanism
        // - Set up journal recovery
        Ok(Self)
    }
}

impl FileSystem for Gfs2FileSystem {
    fn fs_type(&self) -> FileSystemType {
        // GFS2 not yet in FileSystemType enum; would add it
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
