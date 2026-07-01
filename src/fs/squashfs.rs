//! SquashFS read-only filesystem implementation.
//!
//! Stub scaffold for SquashFS mount support. Full port from linux-master fs/squashfs/
//! would include zlib/lz4/xz decompression, metadata parsing, and inode enumeration.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::vec::Vec;

/// SquashFS filesystem stub.
#[derive(Debug)]
pub struct SquashfsFileSystem;

impl SquashfsFileSystem {
    /// Create a new SquashFS filesystem instance.
    pub fn new(_device_id: u32) -> FsResult<Self> {
        // TODO: port from linux-master fs/squashfs
        Err(FsError::NotSupported)
    }
}

impl FileSystem for SquashfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::SquashFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        // TODO: port from linux-master fs/squashfs
        Err(FsError::NotSupported)
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // SquashFS is read-only
        Err(FsError::ReadOnly)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/squashfs
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/squashfs
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // SquashFS is read-only
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, _inode: InodeNumber) -> FsResult<FileMetadata> {
        // TODO: port from linux-master fs/squashfs
        Err(FsError::NotSupported)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // SquashFS is read-only
        Err(FsError::ReadOnly)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // SquashFS is read-only
        Err(FsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        // SquashFS is read-only
        Err(FsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        // SquashFS is read-only
        Err(FsError::ReadOnly)
    }

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        // TODO: port from linux-master fs/squashfs
        Err(FsError::NotSupported)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // SquashFS is read-only
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        // SquashFS is read-only
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        // TODO: port from linux-master fs/squashfs
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
