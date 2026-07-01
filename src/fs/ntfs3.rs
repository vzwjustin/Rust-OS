//! NTFS3 filesystem implementation.
//!
//! Stub scaffold for NTFS3 mount support. Full port from linux-master fs/ntfs3/
//! would include MFT parsing, extent allocation, stream handling, and security descriptors.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::vec::Vec;

/// NTFS3 filesystem stub.
#[derive(Debug)]
pub struct Ntfs3FileSystem;

impl Ntfs3FileSystem {
    /// Create a new NTFS3 filesystem instance.
    pub fn new(_device_id: u32) -> FsResult<Self> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }
}

impl FileSystem for Ntfs3FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Ntfs3
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn metadata(&self, _inode: InodeNumber) -> FsResult<FileMetadata> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        // TODO: port from linux-master fs/ntfs3
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
