//! File Descriptor Management
//!
//! This module manages open file descriptors for the VFS layer.

use super::{InodeOps, OpenFlags, VfsError, VfsResult};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;

/// Open file descriptor
pub struct FileDescriptor {
    /// Inode this descriptor refers to
    pub inode: Arc<dyn InodeOps>,
    /// Open flags
    pub flags: OpenFlags,
    /// Current file offset
    pub offset: u64,
}

impl FileDescriptor {
    /// Create a new file descriptor
    pub fn new(inode: Arc<dyn InodeOps>, flags: OpenFlags) -> Self {
        Self {
            inode,
            flags,
            offset: 0,
        }
    }
}

/// Open file table
///
/// Manages all open file descriptors in the system.
pub struct OpenFileTable {
    /// Map of file descriptor to open file
    files: BTreeMap<i32, FileDescriptor>,
    /// Next available file descriptor
    next_fd: i32,
}

impl OpenFileTable {
    /// Maximum number of open files
    const MAX_FILES: i32 = 1024;

    /// Create a new empty file table
    pub const fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            next_fd: 3, // 0, 1, 2 reserved for stdin, stdout, stderr
        }
    }

    /// Insert a new file descriptor
    pub fn insert(&mut self, file: FileDescriptor) -> VfsResult<i32> {
        if self.files.len() >= Self::MAX_FILES as usize {
            return Err(VfsError::TooManyFiles);
        }

        let fd = self.allocate_fd();
        self.files.insert(fd, file);
        Ok(fd)
    }

    /// Insert at a specific fd number
    pub fn insert_at(&mut self, fd: i32, file: FileDescriptor) -> VfsResult<()> {
        if fd < 0 {
            return Err(VfsError::InvalidArgument);
        }

        if self.files.len() >= Self::MAX_FILES as usize && !self.files.contains_key(&fd) {
            return Err(VfsError::TooManyFiles);
        }

        self.files.insert(fd, file);
        Ok(())
    }

    /// Get a file descriptor (immutable)
    pub fn get(&self, fd: i32) -> VfsResult<&FileDescriptor> {
        self.files.get(&fd).ok_or(VfsError::BadFileDescriptor)
    }

    /// Get a file descriptor (mutable)
    pub fn get_mut(&mut self, fd: i32) -> VfsResult<&mut FileDescriptor> {
        self.files.get_mut(&fd).ok_or(VfsError::BadFileDescriptor)
    }

    /// Remove a file descriptor
    pub fn remove(&mut self, fd: i32) -> VfsResult<()> {
        self.files.remove(&fd).ok_or(VfsError::BadFileDescriptor)?;
        Ok(())
    }

    /// Duplicate a file descriptor
    pub fn duplicate(&mut self, fd: i32) -> VfsResult<i32> {
        let file = self.get(fd)?;
        let new_file = FileDescriptor {
            inode: Arc::clone(&file.inode),
            flags: file.flags,
            offset: file.offset,
        };

        self.insert(new_file)
    }

    /// Duplicate a file descriptor to a specific fd number
    pub fn duplicate_to(&mut self, oldfd: i32, newfd: i32) -> VfsResult<i32> {
        if oldfd == newfd {
            // Verify oldfd exists
            self.get(oldfd)?;
            return Ok(newfd);
        }

        let file = self.get(oldfd)?;
        let new_file = FileDescriptor {
            inode: Arc::clone(&file.inode),
            flags: file.flags,
            offset: file.offset,
        };

        // Close newfd if it exists
        let _ = self.remove(newfd);

        self.insert_at(newfd, new_file)?;
        Ok(newfd)
    }

    /// Allocate a new file descriptor number
    fn allocate_fd(&mut self) -> i32 {
        loop {
            let fd = self.next_fd;
            self.next_fd += 1;

            if self.next_fd >= Self::MAX_FILES {
                self.next_fd = 3; // Wrap around
            }

            if !self.files.contains_key(&fd) {
                return fd;
            }
        }
    }
}
