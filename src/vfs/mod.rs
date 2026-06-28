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

pub mod devfs;
pub mod file_descriptor;
pub mod procfs;
pub mod ramfs;

#[cfg(test)]
pub mod examples;

pub use file_descriptor::{FdKind, FileDescriptor, OpenFileTable};

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

    /// Read the target path of a symbolic link (default: not supported)
    fn read_symlink_target(&self) -> VfsResult<alloc::string::String> {
        Err(VfsError::NotSupported)
    }

    /// Attach an existing inode as a directory entry (device nodes, etc.)
    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    /// Write the target path for a symbolic link (default: not supported)
    fn write_symlink_target(&self, _target: &str) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    /// Change file permissions (default: not supported)
    fn set_mode(&self, _mode: u32) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    /// Change file owner and group (default: not supported)
    fn set_owner(&self, _uid: u32, _gid: u32) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }
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
        if !self.mounts.read().is_empty() {
            return Ok(());
        }

        // Create root ramfs
        let root_fs = ramfs::RamFs::new();
        let root_sb = Arc::new(root_fs);

        // Mount at "/"
        let mut mounts = self.mounts.write();
        mounts.push(MountPoint {
            path: String::from("/"),
            sb: root_sb.clone(),
        });

        drop(mounts);

        // Standard Linux paths expected by userspace
        let root = root_sb.root();
        let _ = root.create("tmp", InodeType::Directory, 0o1777);
        let _ = root.create("dev", InodeType::Directory, 0o755);
        let _ = root.create("sys", InodeType::Directory, 0o555);
        let _ = root.create("run", InodeType::Directory, 0o755);
        let _ = root.create("var", InodeType::Directory, 0o755);
        let _ = root.create("home", InodeType::Directory, 0o755);
        let _ = root.create("etc", InodeType::Directory, 0o755);
        let _ = root.create("bin", InodeType::Directory, 0o755);
        let _ = procfs::install_proc(root.clone());
        let _ = devfs::install_dev(root);

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

    /// Unmount a filesystem at the given path
    pub fn umount(&self, path: &str) -> VfsResult<()> {
        if path == "/" {
            return Err(VfsError::InvalidArgument);
        }

        let mut mounts = self.mounts.write();
        let pos = mounts.iter().position(|m| m.path == path);
        match pos {
            Some(i) => {
                mounts.remove(i);
                Ok(())
            }
            None => Err(VfsError::NotFound),
        }
    }

    /// Return the directory path associated with an open directory fd.
    pub fn fd_directory_path(&self, fd: i32) -> VfsResult<String> {
        let file_table = self.file_table.lock();
        let file_desc = file_table.get(fd)?;
        match &file_desc.kind {
            FdKind::Directory { path } => Ok(path.clone()),
            _ => Err(VfsError::NotDirectory),
        }
    }

    /// Get filesystem statistics for a path
    pub fn statfs(&self, path: &str) -> VfsResult<StatFs> {
        let mounts = self.mounts.read();
        let mount = mounts
            .iter()
            .filter(|m| path.starts_with(&m.path))
            .max_by_key(|m| m.path.len());
        match mount {
            Some(m) => m.sb.statfs(),
            None => Err(VfsError::NotFound),
        }
    }

    /// Sync all mounted filesystems
    pub fn sync_all(&self) -> VfsResult<()> {
        let mounts = self.mounts.read();
        for m in mounts.iter() {
            m.sb.sync_fs()?;
        }
        Ok(())
    }

    /// Resolve a path to an inode
    fn resolve_path(&self, path: &str) -> VfsResult<Arc<dyn InodeOps>> {
        if path.is_empty() {
            return Err(VfsError::InvalidArgument);
        }

        let mounts = self.mounts.read();

        // Find the mount point (longest matching prefix)
        let mount = mounts
            .iter()
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
        let rel_path = path
            .strip_prefix(&mount.path)
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
        let kind = if inode.inode_type() == InodeType::Directory {
            FdKind::Directory {
                path: String::from(path),
            }
        } else {
            FdKind::Regular
        };
        let fd = file_table.insert(FileDescriptor::with_kind(inode, flags, kind))?;

        Ok(fd)
    }

    /// Allocate a VFS fd for a special (non-regular) object.
    pub fn open_special(
        &self,
        inode: Arc<dyn InodeOps>,
        flags: OpenFlags,
        kind: FdKind,
    ) -> VfsResult<i32> {
        let mut file_table = self.file_table.lock();
        file_table.insert(FileDescriptor::with_kind(inode, flags, kind))
    }

    /// Get fd kind for poll/read dispatch.
    pub fn fd_kind(&self, fd: i32) -> VfsResult<FdKind> {
        let file_table = self.file_table.lock();
        file_table.kind(fd)
    }

    /// Snapshot open fds for syscall tracing.
    pub fn open_fd_snapshot(&self) -> Vec<(i32, FdKind)> {
        let file_table = self.file_table.lock();
        file_table.snapshot()
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

    /// Read from a file descriptor at a given offset without changing the file position
    pub fn pread(&self, fd: i32, buf: &mut [u8], offset: u64) -> VfsResult<usize> {
        let file_table = self.file_table.lock();
        let file_desc = file_table.get(fd)?;

        if !file_desc.flags.is_readable() {
            return Err(VfsError::PermissionDenied);
        }

        file_desc.inode.read_at(offset, buf)
    }

    /// Write to a file descriptor at a given offset without changing the file position
    pub fn pwrite(&self, fd: i32, buf: &[u8], offset: u64) -> VfsResult<usize> {
        let file_table = self.file_table.lock();
        let file_desc = file_table.get(fd)?;

        if !file_desc.flags.is_writable() {
            return Err(VfsError::PermissionDenied);
        }

        file_desc.inode.write_at(offset, buf)
    }

    /// Get the inode for a file descriptor (for ftruncate etc.)
    pub fn fd_inode(&self, fd: i32) -> VfsResult<Arc<dyn InodeOps>> {
        let file_table = self.file_table.lock();
        let file_desc = file_table.get(fd)?;
        Ok(Arc::clone(&file_desc.inode))
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

    /// Change file permissions
    pub fn chmod(&self, path: &str, mode: u32) -> VfsResult<()> {
        let inode = self.resolve_path(path)?;
        inode.set_mode(mode)
    }

    /// Change file owner and group
    pub fn chown(&self, path: &str, uid: u32, gid: u32) -> VfsResult<()> {
        let inode = self.resolve_path(path)?;
        inode.set_owner(uid, gid)
    }

    /// Read directory entries
    pub fn readdir(&self, path: &str) -> VfsResult<Vec<DirEntry>> {
        let inode = self.resolve_path(path)?;

        if inode.inode_type() != InodeType::Directory {
            return Err(VfsError::NotDirectory);
        }

        inode.readdir()
    }

    /// Read directory entries via an open directory fd.
    pub fn readdir_fd(&self, fd: i32) -> VfsResult<(Vec<DirEntry>, u64)> {
        let mut file_table = self.file_table.lock();
        let file_desc = file_table.get_mut(fd)?;

        let path = match &file_desc.kind {
            FdKind::Directory { path } => path.clone(),
            _ => return Err(VfsError::NotDirectory),
        };
        let cookie = file_desc.offset;
        drop(file_table);

        let mut entries = self.readdir(&path)?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok((entries, cookie))
    }

    /// Advance directory read cookie on fd.
    pub fn set_dir_cookie(&self, fd: i32, cookie: u64) -> VfsResult<()> {
        let mut file_table = self.file_table.lock();
        let file_desc = file_table.get_mut(fd)?;
        file_desc.offset = cookie;
        Ok(())
    }

    /// Rename a file or directory.
    pub fn rename(&self, oldpath: &str, newpath: &str) -> VfsResult<()> {
        let (old_parent, old_name) = self.resolve_parent(oldpath)?;
        let (new_parent, new_name) = self.resolve_parent(newpath)?;
        old_parent.rename(&old_name, new_parent, &new_name)
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

    /// Create a hard link
    pub fn link(&self, oldpath: &str, newpath: &str) -> VfsResult<()> {
        let target = self.resolve_path(oldpath)?;
        let (new_parent, new_name) = self.resolve_parent(newpath)?;
        new_parent.link(&new_name, target)
    }

    /// Create a symbolic link
    pub fn symlink(&self, target: &str, linkpath: &str) -> VfsResult<()> {
        let (parent, name) = self.resolve_parent(linkpath)?;
        let inode = parent.create(&name, InodeType::Symlink, 0o777)?;
        inode.write_symlink_target(target)
    }

    /// Read the target of a symbolic link
    pub fn readlink(&self, path: &str) -> VfsResult<alloc::string::String> {
        let inode = self.resolve_path(path)?;
        if inode.inode_type() != InodeType::Symlink {
            return Err(VfsError::NotSupported);
        }
        inode.read_symlink_target()
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

/// Read directory entries by open fd
pub fn vfs_readdir_fd(fd: i32) -> VfsResult<(Vec<DirEntry>, u64)> {
    VFS.readdir_fd(fd)
}

/// Set directory read cookie
pub fn vfs_set_dir_cookie(fd: i32, cookie: u64) -> VfsResult<()> {
    VFS.set_dir_cookie(fd, cookie)
}

/// Get fd kind
pub fn vfs_fd_kind(fd: i32) -> VfsResult<FdKind> {
    VFS.fd_kind(fd)
}

/// Open a special fd
pub fn vfs_open_special(inode: Arc<dyn InodeOps>, flags: u32, kind: FdKind) -> VfsResult<i32> {
    VFS.open_special(inode, OpenFlags::new(flags), kind)
}

/// Rename paths
pub fn vfs_rename(oldpath: &str, newpath: &str) -> VfsResult<()> {
    VFS.rename(oldpath, newpath)
}

/// Sync a file descriptor
pub fn vfs_fsync(fd: i32) -> VfsResult<()> {
    VFS.fsync(fd)
}

/// Create a hard link
pub fn vfs_link(oldpath: &str, newpath: &str) -> VfsResult<()> {
    VFS.link(oldpath, newpath)
}

/// Create a symbolic link
pub fn vfs_symlink(target: &str, linkpath: &str) -> VfsResult<()> {
    VFS.symlink(target, linkpath)
}

/// Read the target of a symbolic link
pub fn vfs_readlink(path: &str) -> VfsResult<alloc::string::String> {
    VFS.readlink(path)
}

/// Read from fd at offset without changing file position
pub fn vfs_pread(fd: i32, buf: &mut [u8], offset: u64) -> VfsResult<usize> {
    VFS.pread(fd, buf, offset)
}

/// Write to fd at offset without changing file position
pub fn vfs_pwrite(fd: i32, buf: &[u8], offset: u64) -> VfsResult<usize> {
    VFS.pwrite(fd, buf, offset)
}

/// Truncate file by fd
pub fn vfs_ftruncate(fd: i32, size: u64) -> VfsResult<()> {
    let inode = VFS.fd_inode(fd)?;
    inode.truncate(size)
}

/// Get filesystem statistics for a path
pub fn vfs_statfs(path: &str) -> VfsResult<StatFs> {
    VFS.statfs(path)
}

/// Change file permissions
pub fn vfs_chmod(path: &str, mode: u32) -> VfsResult<()> {
    VFS.chmod(path, mode)
}

/// Change file owner and group
pub fn vfs_chown(path: &str, uid: u32, gid: u32) -> VfsResult<()> {
    VFS.chown(path, uid, gid)
}

/// Change file permissions by fd
pub fn vfs_fchmod(fd: i32, mode: u32) -> VfsResult<()> {
    let inode = VFS.fd_inode(fd)?;
    inode.set_mode(mode)
}

/// Change file owner and group by fd
pub fn vfs_fchown(fd: i32, uid: u32, gid: u32) -> VfsResult<()> {
    let inode = VFS.fd_inode(fd)?;
    inode.set_owner(uid, gid)
}

/// Mount a filesystem at the given path
pub fn vfs_mount(path: &str, sb: Arc<dyn SuperblockOps>) -> VfsResult<()> {
    VFS.mount(path, sb)
}

/// Unmount a filesystem at the given path
pub fn vfs_umount(path: &str) -> VfsResult<()> {
    VFS.umount(path)
}

/// Return the directory path for an open directory fd
pub fn vfs_fd_directory_path(fd: i32) -> VfsResult<String> {
    VFS.fd_directory_path(fd)
}

/// Get extended attribute value (returns NotSupported when unavailable)
pub fn vfs_getxattr(path: &str, name: &str) -> VfsResult<alloc::vec::Vec<u8>> {
    let _ = (path, name);
    Err(VfsError::NotSupported)
}

/// Set extended attribute value (returns NotSupported when unavailable)
pub fn vfs_setxattr(path: &str, name: &str, value: &[u8], create: bool) -> VfsResult<()> {
    let _ = (path, name, value, create);
    Err(VfsError::NotSupported)
}

/// List extended attribute names (returns NotSupported when unavailable)
pub fn vfs_listxattr(path: &str) -> VfsResult<alloc::vec::Vec<u8>> {
    let _ = path;
    Err(VfsError::NotSupported)
}

/// Remove extended attribute (returns NotSupported when unavailable)
pub fn vfs_removexattr(path: &str, name: &str) -> VfsResult<()> {
    let _ = (path, name);
    Err(VfsError::NotSupported)
}

/// Get extended attribute by fd (returns NotSupported when unavailable)
pub fn vfs_fgetxattr(_fd: i32, name: &str) -> VfsResult<alloc::vec::Vec<u8>> {
    let _ = name;
    Err(VfsError::NotSupported)
}

/// Set extended attribute by fd (returns NotSupported when unavailable)
pub fn vfs_fsetxattr(_fd: i32, name: &str, value: &[u8], create: bool) -> VfsResult<()> {
    let _ = (name, value, create);
    Err(VfsError::NotSupported)
}

/// List extended attributes by fd (returns NotSupported when unavailable)
pub fn vfs_flistxattr(_fd: i32) -> VfsResult<alloc::vec::Vec<u8>> {
    Err(VfsError::NotSupported)
}

/// Remove extended attribute by fd (returns NotSupported when unavailable)
pub fn vfs_fremovexattr(_fd: i32, name: &str) -> VfsResult<()> {
    let _ = name;
    Err(VfsError::NotSupported)
}
