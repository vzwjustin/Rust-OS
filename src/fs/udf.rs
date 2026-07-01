//! UDF (Universal Disk Format) optical media filesystem implementation.
//!
//! Stub scaffold for UDF mount support. UDF is commonly used on DVDs, Blu-rays, and
//! other optical media. Full port from linux-master fs/udf/ would include descriptor
//! parsing, extent handling, and metadata block processing similar to isofs.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::vec::Vec;

/// UDF filesystem stub.
#[derive(Debug)]
pub struct UdfFileSystem;

impl UdfFileSystem {
    /// Create a new UDF filesystem instance.
    pub fn new(_device_id: u32) -> FsResult<Self> {
        // TODO: port from linux-master fs/udf
        Err(FsError::NotSupported)
    }
}

impl FileSystem for UdfFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Udf
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        // TODO: port from linux-master fs/udf
        Err(FsError::NotSupported)
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // UDF on optical media is typically read-only
        Err(FsError::ReadOnly)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/udf
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/udf
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // UDF on optical media is typically read-only
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, _inode: InodeNumber) -> FsResult<FileMetadata> {
        // TODO: port from linux-master fs/udf
        Err(FsError::NotSupported)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // UDF on optical media is typically read-only
        Err(FsError::ReadOnly)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // UDF on optical media is typically read-only
        Err(FsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        // UDF on optical media is typically read-only
        Err(FsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        // UDF on optical media is typically read-only
        Err(FsError::ReadOnly)
    }

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        // TODO: port from linux-master fs/udf
        Err(FsError::NotSupported)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // UDF on optical media is typically read-only
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        // UDF on optical media is typically read-only
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        // TODO: port from linux-master fs/udf
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
