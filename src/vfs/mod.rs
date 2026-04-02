//! Virtual File System (VFS) Layer
//!
//! This module provides a unified interface for all file system operations in RustOS.
//! It defines the core abstractions (Inode, Dentry, Superblock) and provides a
//! pluggable filesystem interface.

#![allow(dead_code)]

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, RwLock};

pub mod ramfs;
pub mod file_descriptor;

#[cfg(test)]
pub mod examples;

pub use file_descriptor::{FileDescriptor, OpenFileTable};

/// VFS error type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    /// File or directory not found
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// File or directory already exists
    AlreadyExists,
    /// Not a directory
    NotDirectory,
    /// Is a directory
    IsDirectory,
    /// Invalid argument
    InvalidArgument,
    /// I/O error
    IoError,
    /// No space left on device
    NoSpace,
    /// Too many open files
    TooManyFiles,
    /// Bad file descriptor
    BadFileDescriptor,
    /// Invalid seek operation
    InvalidSeek,
    /// Name too long
    NameTooLong,
    /// Cross-device link
    CrossDevice,
    /// Read-only filesystem
    ReadOnly,
    /// Operation not supported
    NotSupported,
}

pub type VfsResult<T> = Result<T, VfsError>;

/// Inode type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodeType {
    /// Regular file
    File,
    /// Directory
    Directory,
    /// Character device
    CharDevice,
    /// Block device
    BlockDevice,
    /// Named pipe (FIFO)
    Fifo,
    /// Unix domain socket
    Socket,
    /// Symbolic link
    Symlink,
}

/// File access mode flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenFlags {
    bits: u32,
}

impl OpenFlags {
    pub const RDONLY: u32 = 0x00;
    pub const WRONLY: u32 = 0x01;
    pub const RDWR: u32 = 0x02;
    pub const CREAT: u32 = 0x100;
    pub const EXCL: u32 = 0x200;
    pub const TRUNC: u32 = 0x400;
    pub const APPEND: u32 = 0x800;
    pub const NONBLOCK: u32 = 0x1000;
    pub const DIRECTORY: u32 = 0x10000;

    pub const fn new(bits: u32) -> Self {
        Self { bits }
    }

    pub const fn bits(&self) -> u32 {
        self.bits
    }

    pub fn is_readable(&self) -> bool {
        (self.bits & 0x03) != Self::WRONLY
    }

    pub fn is_writable(&self) -> bool {
        (self.bits & 0x03) != Self::RDONLY
    }

    pub fn has_flag(&self, flag: u32) -> bool {
        (self.bits & flag) != 0
    }
}

/// Seek position
#[derive(Debug, Clone, Copy)]
pub enum SeekFrom {
    /// Seek from start of file
    Start(u64),
    /// Seek from current position
    Current(i64),
    /// Seek from end of file
    End(i64),
}

/// File statistics
#[derive(Debug, Clone, Copy)]
pub struct Stat {
    /// Inode number
    pub ino: u64,
    /// File type
    pub inode_type: InodeType,
    /// File size in bytes
    pub size: u64,
    /// Block size for I/O
    pub blksize: u64,
    /// Number of 512B blocks allocated
    pub blocks: u64,
    /// Access permissions
    pub mode: u32,
    /// Number of hard links
    pub nlink: u32,
    /// User ID of owner
    pub uid: u32,
    /// Group ID of owner
    pub gid: u32,
    /// Device ID (for special files)
    pub rdev: u64,
    /// Time of last access (seconds since epoch)
    pub atime: u64,
    /// Time of last modification (seconds since epoch)
    pub mtime: u64,
    /// Time of last status change (seconds since epoch)
    pub ctime: u64,
}

impl Default for Stat {
    fn default() -> Self {
        Self {
            ino: 0,
            inode_type: InodeType::File,
            size: 0,
            blksize: 4096,
            blocks: 0,
            mode: 0o644,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        }
    }
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Inode number
    pub ino: u64,
    /// Entry name
    pub name: String,
    /// File type
    pub inode_type: InodeType,
}

/// Inode operations trait
///
/// Defines the operations that can be performed on an inode.
pub trait InodeOps: Send + Sync {
    /// Read data from the inode
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize>;

    /// Write data to the inode
    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize>;

    /// Get inode metadata
    fn stat(&self) -> VfsResult<Stat>;

    /// Truncate or extend the file to the specified size
    fn truncate(&self, size: u64) -> VfsResult<()>;

    /// Sync file data and metadata to storage
    fn sync(&self) -> VfsResult<()>;

    /// Lookup a child entry in a directory
    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn InodeOps>>;

    /// Create a new file in this directory
    fn create(&self, name: &str, inode_type: InodeType, mode: u32) -> VfsResult<Arc<dyn InodeOps>>;

    /// Remove an entry from this directory
    fn unlink(&self, name: &str) -> VfsResult<()>;

    /// Create a hard link
    fn link(&self, name: &str, target: Arc<dyn InodeOps>) -> VfsResult<()>;

    /// Rename an entry
    fn rename(&self, old_name: &str, new_dir: Arc<dyn InodeOps>, new_name: &str) -> VfsResult<()>;

    /// Read directory entries
    fn readdir(&self) -> VfsResult<Vec<DirEntry>>;

    /// Get the inode type
    fn inode_type(&self) -> InodeType;
}

/// Superblock operations trait
///
/// Represents a mounted filesystem instance.
pub trait SuperblockOps: Send + Sync {
    /// Get the root inode of this filesystem
    fn root(&self) -> Arc<dyn InodeOps>;

    /// Sync all filesystem metadata
    fn sync_fs(&self) -> VfsResult<()>;

    /// Get filesystem statistics
    fn statfs(&self) -> VfsResult<StatFs>;
}

/// Filesystem statistics
#[derive(Debug, Clone, Copy)]
pub struct StatFs {
    /// Filesystem type
    pub fs_type: u64,
    /// Block size
    pub block_size: u64,
    /// Total blocks
    pub total_blocks: u64,
    /// Free blocks
    pub free_blocks: u64,
    /// Available blocks
    pub avail_blocks: u64,
    /// Total inodes
    pub total_inodes: u64,
    /// Free inodes
    pub free_inodes: u64,
    /// Maximum filename length
    pub max_name_len: u64,
}

/// VFS mount point
struct MountPoint {
    /// Mount path
    path: String,
    /// Superblock
    sb: Arc<dyn SuperblockOps>,
}

/// Global VFS state
pub struct Vfs {
    /// Mounted filesystems
    mounts: RwLock<Vec<MountPoint>>,
    /// Global open file table
    file_table: Mutex<OpenFileTable>,
    /// Next inode number
    next_ino: AtomicU64,
}

impl Vfs {
    /// Create a new VFS instance
    pub const fn new() -> Self {
        Self {
            mounts: RwLock::new(Vec::new()),
            file_table: Mutex::new(OpenFileTable::new()),
            next_ino: AtomicU64::new(1),
        }
    }

    /// Initialize VFS with a root filesystem
    pub fn init(&self) -> VfsResult<()> {
        // Create root ramfs
        let root_fs = ramfs::RamFs::new();
        let root_sb = Arc::new(root_fs);

        // Mount at "/"
        let mut mounts = self.mounts.write();
        mounts.push(MountPoint {
            path: String::from("/"),
            sb: root_sb,
        });

        Ok(())
    }

    /// Allocate a new inode number
    pub fn alloc_ino(&self) -> u64 {
        self.next_ino.fetch_add(1, Ordering::SeqCst)
    }

    /// Mount a filesystem at the given path
    pub fn mount(&self, path: &str, sb: Arc<dyn SuperblockOps>) -> VfsResult<()> {
        let mut mounts = self.mounts.write();

        // Check if path already mounted
        if mounts.iter().any(|m| m.path == path) {
            return Err(VfsError::AlreadyExists);
        }

        mounts.push(MountPoint {
            path: String::from(path),
            sb,
        });

        Ok(())
    }

    /// Resolve a path to an inode
    fn resolve_path(&self, path: &str) -> VfsResult<Arc<dyn InodeOps>> {
        if path.is_empty() {
            return Err(VfsError::InvalidArgument);
        }

        let mounts = self.mounts.read();

        // Find the mount point (longest matching prefix)
        let mount = mounts.iter()
            .filter(|m| path.starts_with(&m.path))
            .max_by_key(|m| m.path.len())
            .ok_or(VfsError::NotFound)?;

        // Get root inode of the mount
        let mut current = mount.sb.root();

        // If path is just the mount point, return root
        if path == mount.path {
            return Ok(current);
        }

        // Strip mount prefix and leading slash
        let rel_path = path.strip_prefix(&mount.path)
            .unwrap_or(path)
            .trim_start_matches('/');

        // Walk the path
        if !rel_path.is_empty() {
            for component in rel_path.split('/') {
                if component.is_empty() || component == "." {
                    continue;
                }

                if component == ".." {
                    // TODO: Handle parent directory traversal
                    continue;
                }

                // Lookup next component
                current = current.lookup(component)?;
            }
        }

        Ok(current)
    }

    /// Resolve parent directory and filename from path
    fn resolve_parent(&self, path: &str) -> VfsResult<(Arc<dyn InodeOps>, String)> {
        let path = path.trim_end_matches('/');

        if let Some(pos) = path.rfind('/') {
            let parent_path = if pos == 0 { "/" } else { &path[..pos] };
            let filename = &path[pos + 1..];

            if filename.is_empty() {
                return Err(VfsError::InvalidArgument);
            }

            let parent = self.resolve_path(parent_path)?;
            Ok((parent, String::from(filename)))
        } else {
            // Relative path, use root for now
            let root = self.resolve_path("/")?;
            Ok((root, String::from(path)))
        }
    }

    /// Open a file
    pub fn open(&self, path: &str, flags: OpenFlags, mode: u32) -> VfsResult<i32> {
        let inode = if flags.has_flag(OpenFlags::CREAT) {
            // Try to resolve existing file
            match self.resolve_path(path) {
                Ok(inode) => {
                    if flags.has_flag(OpenFlags::EXCL) {
                        return Err(VfsError::AlreadyExists);
                    }
                    inode
                }
                Err(VfsError::NotFound) => {
                    // Create new file
                    let (parent, filename) = self.resolve_parent(path)?;
                    parent.create(&filename, InodeType::File, mode)?
                }
                Err(e) => return Err(e),
            }
        } else {
            self.resolve_path(path)?
        };

        // Check directory constraint
        if flags.has_flag(OpenFlags::DIRECTORY) && inode.inode_type() != InodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        // Truncate if requested
        if flags.has_flag(OpenFlags::TRUNC) && flags.is_writable() {
            inode.truncate(0)?;
        }

        // Add to file table
        let mut file_table = self.file_table.lock();
        let fd = file_table.insert(FileDescriptor::new(inode, flags))?;

        Ok(fd)
    }

    /// Close a file descriptor
    pub fn close(&self, fd: i32) -> VfsResult<()> {
        let mut file_table = self.file_table.lock();
        file_table.remove(fd)
    }

    /// Read from a file descriptor
    pub fn read(&self, fd: i32, buf: &mut [u8]) -> VfsResult<usize> {
        let mut file_table = self.file_table.lock();
        let file_desc = file_table.get_mut(fd)?;

        if !file_desc.flags.is_readable() {
            return Err(VfsError::PermissionDenied);
        }

        let bytes_read = file_desc.inode.read_at(file_desc.offset, buf)?;
        file_desc.offset += bytes_read as u64;

        Ok(bytes_read)
    }

    /// Write to a file descriptor
    pub fn write(&self, fd: i32, buf: &[u8]) -> VfsResult<usize> {
        let mut file_table = self.file_table.lock();
        let file_desc = file_table.get_mut(fd)?;

        if !file_desc.flags.is_writable() {
            return Err(VfsError::PermissionDenied);
        }

        // Handle append mode
        if file_desc.flags.has_flag(OpenFlags::APPEND) {
            let stat = file_desc.inode.stat()?;
            file_desc.offset = stat.size;
        }

        let bytes_written = file_desc.inode.write_at(file_desc.offset, buf)?;
        file_desc.offset += bytes_written as u64;

        Ok(bytes_written)
    }

    /// Seek in a file descriptor
    pub fn seek(&self, fd: i32, offset: SeekFrom) -> VfsResult<u64> {
        let mut file_table = self.file_table.lock();
        let file_desc = file_table.get_mut(fd)?;

        let new_offset = match offset {
            SeekFrom::Start(off) => off as i64,
            SeekFrom::Current(off) => file_desc.offset as i64 + off,
            SeekFrom::End(off) => {
                let stat = file_desc.inode.stat()?;
                stat.size as i64 + off
            }
        };

        if new_offset < 0 {
            return Err(VfsError::InvalidSeek);
        }

        file_desc.offset = new_offset as u64;
        Ok(file_desc.offset)
    }

    /// Get file statistics
    pub fn stat(&self, path: &str) -> VfsResult<Stat> {
        let inode = self.resolve_path(path)?;
        inode.stat()
    }

    /// Look up an inode by path
    pub fn lookup(&self, path: &str) -> VfsResult<Arc<dyn InodeOps>> {
        self.resolve_path(path)
    }

    /// Get file statistics by file descriptor
    pub fn fstat(&self, fd: i32) -> VfsResult<Stat> {
        let file_table = self.file_table.lock();
        let file_desc = file_table.get(fd)?;
        file_desc.inode.stat()
    }

    /// Create a directory
    pub fn mkdir(&self, path: &str, mode: u32) -> VfsResult<()> {
        let (parent, dirname) = self.resolve_parent(path)?;
        parent.create(&dirname, InodeType::Directory, mode)?;
        Ok(())
    }

    /// Remove a directory
    pub fn rmdir(&self, path: &str) -> VfsResult<()> {
        let (parent, dirname) = self.resolve_parent(path)?;
        let inode = parent.lookup(&dirname)?;

        // Verify it's a directory
        if inode.inode_type() != InodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        // Verify it's empty
        let entries = inode.readdir()?;
        if !entries.is_empty() {
            return Err(VfsError::NotSupported); // Should be ENOTEMPTY
        }

        parent.unlink(&dirname)
    }

    /// Remove a file
    pub fn unlink(&self, path: &str) -> VfsResult<()> {
        let (parent, filename) = self.resolve_parent(path)?;
        parent.unlink(&filename)
    }

    /// Read directory entries
    pub fn readdir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let inode = self.resolve_path(path)?;

        if inode.inode_type() != InodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        inode.readdir()
    }

    /// Sync a file descriptor
    pub fn fsync(&self, fd: i32) -> VfsResult<()> {
        let file_table = self.file_table.lock();
        let file_desc = file_table.get(fd)?;
        file_desc.inode.sync()
    }

    /// Duplicate a file descriptor
    pub fn dup(&self, fd: i32) -> VfsResult<i32> {
        let mut file_table = self.file_table.lock();
        file_table.duplicate(fd)
    }

    /// Duplicate a file descriptor to a specific fd number
    pub fn dup2(&self, oldfd: i32, newfd: i32) -> VfsResult<i32> {
        let mut file_table = self.file_table.lock();
        file_table.duplicate_to(oldfd, newfd)
    }
}

/// Global VFS instance
static VFS: Vfs = Vfs::new();

/// Get the global VFS instance
pub fn get_vfs() -> &'static Vfs {
    &VFS
}

/// Initialize the VFS system
pub fn init() -> VfsResult<()> {
    VFS.init()
}

// Public API functions

/// Open a file
pub fn vfs_open(path: &str, flags: u32, mode: u32) -> VfsResult<i32> {
    VFS.open(path, OpenFlags::new(flags), mode)
}

/// Close a file descriptor
pub fn vfs_close(fd: i32) -> VfsResult<()> {
    VFS.close(fd)
}

/// Read from a file descriptor
pub fn vfs_read(fd: i32, buf: &mut [u8]) -> VfsResult<usize> {
    VFS.read(fd, buf)
}

/// Write to a file descriptor
pub fn vfs_write(fd: i32, buf: &[u8]) -> VfsResult<usize> {
    VFS.write(fd, buf)
}

/// Seek in a file descriptor
pub fn vfs_seek(fd: i32, offset: SeekFrom) -> VfsResult<u64> {
    VFS.seek(fd, offset)
}

/// Get file statistics
pub fn vfs_stat(path: &str) -> VfsResult<Stat> {
    VFS.stat(path)
}

/// Get file statistics by file descriptor
pub fn vfs_fstat(fd: i32) -> VfsResult<Stat> {
    VFS.fstat(fd)
}

/// Create a directory
pub fn vfs_mkdir(path: &str, mode: u32) -> VfsResult<()> {
    VFS.mkdir(path, mode)
}

/// Remove a directory
pub fn vfs_rmdir(path: &str) -> VfsResult<()> {
    VFS.rmdir(path)
}

/// Remove a file
pub fn vfs_unlink(path: &str) -> VfsResult<()> {
    VFS.unlink(path)
}

/// Read directory entries
pub fn vfs_readdir(path: &str) -> VfsResult<Vec<DirEntry>> {
    VFS.readdir(path)
}

/// Sync a file descriptor
pub fn vfs_fsync(fd: i32) -> VfsResult<()> {
    VFS.fsync(fd)
}
