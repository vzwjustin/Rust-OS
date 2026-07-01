//! ROMFS read-only filesystem implementation.
//!
//! Stub scaffold for ROMFS mount support. ROMFS is a simple, read-only filesystem
//! commonly used in embedded systems and initramfs. Full port from linux-master fs/romfs/
//! would include superblock validation, inode enumeration, and extent map parsing.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::vec::Vec;

/// ROMFS filesystem stub.
#[derive(Debug)]
pub struct RomfsFileSystem;

impl RomfsFileSystem {
    /// Create a new ROMFS filesystem instance.
    pub fn new(_device_id: u32) -> FsResult<Self> {
        // TODO: port from linux-master fs/romfs
        Err(FsError::NotSupported)
    }
}

impl FileSystem for RomfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::RomFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        // TODO: port from linux-master fs/romfs
        Err(FsError::NotSupported)
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // ROMFS is read-only
        Err(FsError::ReadOnly)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        // TODO: port from linux-master fs/romfs
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        // TODO: port from linux-master fs/romfs
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // ROMFS is read-only
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, _inode: InodeNumber) -> FsResult<FileMetadata> {
        // TODO: port from linux-master fs/romfs
        Err(FsError::NotSupported)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        // ROMFS is read-only
        Err(FsError::ReadOnly)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // ROMFS is read-only
        Err(FsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        // ROMFS is read-only
        Err(FsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        // ROMFS is read-only
        Err(FsError::ReadOnly)
    }

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        // TODO: port from linux-master fs/romfs
        Err(FsError::NotSupported)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        // ROMFS is read-only
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        // ROMFS is read-only
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        // TODO: port from linux-master fs/romfs
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
