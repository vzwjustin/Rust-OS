//! CRAMFS (Compressed ROM File System) stub implementation
//!
//! CRAMFS is a read-only, compressed filesystem optimized for embedded systems and
//! bootable media. This is a compile-clean stub; full implementation would require
//! decompression (zlib), block mapping, and inode unpacking.
//! See: linux-master/fs/cramfs/

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::vec::Vec;

/// CRAMFS filesystem (stub)
#[derive(Debug)]
pub struct CramfsFileSystem;

impl CramfsFileSystem {
    /// Create a new CRAMFS filesystem instance
    pub fn new() -> FsResult<Self> {
        // TODO: port from linux-master fs/cramfs/
        // - Parse superblock and verify magic/version
        // - Build inode/block mapping tables
        // - Initialize decompression context
        Ok(Self)
    }
}

impl FileSystem for CramfsFileSystem {
    fn fs_type(&self) -> FileSystemType {
        // CRAMFS not yet in FileSystemType enum; would add it
        FileSystemType::RamFs // Stub
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        Err(FsError::NotSupported)
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // Read-only filesystem
        Err(FsError::ReadOnly)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // Read-only filesystem
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, _inode: InodeNumber) -> FsResult<FileMetadata> {
        Err(FsError::NotSupported)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        Err(FsError::NotSupported)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, _path: &str) -> FsResult<alloc::string::String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}
