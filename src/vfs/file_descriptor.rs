//! File Descriptor Management
//!
//! This module manages open file descriptors for the VFS layer.

use super::{InodeOps, OpenFlags, VfsError, VfsResult};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;

/// Non-regular file descriptor kinds tracked alongside VFS inodes.
#[derive(Debug, Clone)]
pub enum FdKind {
    Regular,
    Directory {
        path: String,
    },
    PipeRead(u32),
    PipeWrite(u32),
    EventFd(u32),
    TimerFd(u32),
    Epoll(u32),
    Signalfd(u32),
    Socket(u32),
    Inotify(u32),
    Pidfd(u32),
    IoUring(u32),
    Fanotify(u32),
    FsContext(u32),
    MountObject(u32),
    LandlockRuleset(u32),
    BpfMap(u32),
    BpfProg(u32),
    PerfEvent(u32),
    Userfaultfd(u32),
    MemfdSecret(u32),
    Namespace(u32),
    /// /dev/console or /dev/tty
    TtyConsole,
    /// PTY master (/dev/ptmx)
    PtyMaster(u32),
    /// PTY slave (/dev/pts/N)
    PtySlave(u32),
}

impl FdKind {
    pub const fn regular() -> Self {
        Self::Regular
    }
}

/// Open file descriptor
pub struct FileDescriptor {
    /// Inode this descriptor refers to
    pub inode: Arc<dyn InodeOps>,
    /// Open flags
    pub flags: OpenFlags,
    /// Current file offset (also used as directory read cookie)
    pub offset: u64,
    /// Special fd kind for poll/read/write dispatch
    pub kind: FdKind,
    /// VFS path when opened via path-based open (for quota accounting)
    pub path: Option<String>,
}

impl FileDescriptor {
    /// Create a new file descriptor
    pub fn new(inode: Arc<dyn InodeOps>, flags: OpenFlags) -> Self {
        Self {
            inode,
            flags,
            offset: 0,
            kind: FdKind::regular(),
            path: None,
        }
    }

    /// Create a file descriptor with an explicit kind
    pub fn with_kind(inode: Arc<dyn InodeOps>, flags: OpenFlags, kind: FdKind) -> Self {
        Self {
            inode,
            flags,
            offset: 0,
            kind,
            path: None,
        }
    }

    /// Create a path-backed file descriptor.
    pub fn with_path(
        inode: Arc<dyn InodeOps>,
        flags: OpenFlags,
        kind: FdKind,
        path: String,
    ) -> Self {
        Self {
            inode,
            flags,
            offset: 0,
            kind,
            path: Some(path),
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

    /// Get the fd kind if the fd is open
    pub fn kind(&self, fd: i32) -> VfsResult<FdKind> {
        Ok(self.get(fd)?.kind.clone())
    }

    /// Set flags on an existing file descriptor
    pub fn set_flags(&mut self, fd: i32, flags: OpenFlags) -> VfsResult<()> {
        let file = self.get_mut(fd)?;
        file.flags = flags;
        Ok(())
    }

    /// Remove a file descriptor
    pub fn remove(&mut self, fd: i32) -> VfsResult<()> {
        self.files.remove(&fd).ok_or(VfsError::BadFileDescriptor)?;
        Ok(())
    }

    /// Snapshot open file descriptors for debugging.
    pub fn snapshot(&self) -> alloc::vec::Vec<(i32, FdKind)> {
        self.files
            .iter()
            .map(|(&fd, file)| (fd, file.kind.clone()))
            .collect()
    }

    /// Duplicate a file descriptor
    pub fn duplicate(&mut self, fd: i32) -> VfsResult<i32> {
        let file = self.get(fd)?;
        let new_file = FileDescriptor {
            inode: Arc::clone(&file.inode),
            flags: file.flags,
            offset: file.offset,
            kind: file.kind.clone(),
            path: file.path.clone(),
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
            kind: file.kind.clone(),
            path: file.path.clone(),
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
