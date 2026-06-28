//! Unix domain socket nodes for the VFS.
//!
//! These inodes exist so userspace can stat socket paths (e.g. `/run/user/0/bus`)
//! as `S_IFSOCK`. Actual I/O is handled through AF_UNIX syscalls backed by the
//! kernel IPC pipe layer.

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use super::{DirEntry, InodeOps, InodeType, Stat, VfsError, VfsResult};

/// Mode bits for a world-accessible runtime socket (0700 + S_IFSOCK).
const SOCKET_MODE: u32 = 0o140000 | 0o700;

/// A bound Unix domain socket path exposed through the VFS.
pub struct UnixSocketInode {
    ino: u64,
    path: String,
}

impl UnixSocketInode {
    pub fn new(ino: u64, path: &str) -> Arc<Self> {
        Arc::new(Self {
            ino,
            path: String::from(path),
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

impl InodeOps for UnixSocketInode {
    fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::NotSupported)
    }

    fn write_at(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::NotSupported)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::Socket,
            size: 0,
            blksize: 4096,
            blocks: 0,
            mode: SOCKET_MODE,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(
        &self,
        _name: &str,
        _inode_type: InodeType,
        _mode: u32,
    ) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self) -> VfsResult<Vec<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn inode_type(&self) -> InodeType {
        InodeType::Socket
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}
