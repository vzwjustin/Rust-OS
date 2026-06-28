//! Virtual File System (VFS) for RustOS
//!
//! This module implements a Virtual File System layer that provides:
//! - Unified interface for different filesystem types
//! - File and directory operations
//! - Mount point management
//! - File descriptor management
//! - Path resolution and caching

pub mod buffer;
pub mod devfs;
pub mod ext4;
pub mod fat32;
pub mod ramfs;
pub mod vfs;

use alloc::{
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use bitflags::bitflags;
use core::fmt;
use lazy_static::lazy_static;
use spin::{Mutex, RwLock};

/// File descriptor type
pub type FileDescriptor = i32;

/// Inode number type
pub type InodeNumber = u64;

/// File system type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSystemType {
    /// RAM-based filesystem
    RamFs,
    /// Device filesystem
    DevFs,
    /// Ext2 filesystem
    Ext2,
    /// FAT32 filesystem
    Fat32,
    /// Network filesystem
    NetworkFs,
}

impl fmt::Display for FileSystemType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FileSystemType::RamFs => write!(f, "ramfs"),
            FileSystemType::DevFs => write!(f, "devfs"),
            FileSystemType::Ext2 => write!(f, "ext2"),
            FileSystemType::Fat32 => write!(f, "fat32"),
            FileSystemType::NetworkFs => write!(f, "nfs"),
        }
    }
}

/// File type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// Regular file
    Regular,
    /// Directory
    Directory,
    /// Symbolic link
    SymbolicLink,
    /// Character device
    CharacterDevice,
    /// Block device
    BlockDevice,
    /// Named pipe (FIFO)
    NamedPipe,
    /// Unix domain socket
    Socket,
}

/// File permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilePermissions {
    /// Owner read permission
    pub owner_read: bool,
    /// Owner write permission
    pub owner_write: bool,
    /// Owner execute permission
    pub owner_execute: bool,
    /// Group read permission
    pub group_read: bool,
    /// Group write permission
    pub group_write: bool,
    /// Group execute permission
    pub group_execute: bool,
    /// Other read permission
    pub other_read: bool,
    /// Other write permission
    pub other_write: bool,
    /// Other execute permission
    pub other_execute: bool,
}

impl FilePermissions {
    /// Create new permissions with octal mode
    pub fn from_octal(mode: u16) -> Self {
        Self {
            owner_read: (mode & 0o400) != 0,
            owner_write: (mode & 0o200) != 0,
            owner_execute: (mode & 0o100) != 0,
            group_read: (mode & 0o040) != 0,
            group_write: (mode & 0o020) != 0,
            group_execute: (mode & 0o010) != 0,
            other_read: (mode & 0o004) != 0,
            other_write: (mode & 0o002) != 0,
            other_execute: (mode & 0o001) != 0,
        }
    }

    /// Convert to octal mode
    pub fn to_octal(&self) -> u16 {
        let mut mode = 0u16;
        if self.owner_read {
            mode |= 0o400;
        }
        if self.owner_write {
            mode |= 0o200;
        }
        if self.owner_execute {
            mode |= 0o100;
        }
        if self.group_read {
            mode |= 0o040;
        }
        if self.group_write {
            mode |= 0o020;
        }
        if self.group_execute {
            mode |= 0o010;
        }
        if self.other_read {
            mode |= 0o004;
        }
        if self.other_write {
            mode |= 0o002;
        }
        if self.other_execute {
            mode |= 0o001;
        }
        mode
    }

    /// Default permissions for regular files (644)
    pub fn default_file() -> Self {
        Self::from_octal(0o644)
    }

    /// Default permissions for directories (755)
    pub fn default_directory() -> Self {
        Self::from_octal(0o755)
    }
}

/// File metadata
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// Inode number
    pub inode: InodeNumber,
    /// File type
    pub file_type: FileType,
    /// File size in bytes
    pub size: u64,
    /// File permissions
    pub permissions: FilePermissions,
    /// Owner user ID
    pub uid: u32,
    /// Owner group ID
    pub gid: u32,
    /// Creation time (Unix timestamp)
    pub created: u64,
    /// Last modification time
    pub modified: u64,
    /// Last access time
    pub accessed: u64,
    /// Number of hard links
    pub link_count: u32,
    /// Device ID (for device files)
    pub device_id: Option<u32>,
}

impl FileMetadata {
    /// Create new file metadata
    pub fn new(inode: InodeNumber, file_type: FileType, size: u64) -> Self {
        let now = get_current_time();
        Self {
            inode,
            file_type,
            size,
            permissions: match file_type {
                FileType::Directory => FilePermissions::default_directory(),
                _ => FilePermissions::default_file(),
            },
            uid: 0, // Root user
            gid: 0, // Root group
            created: now,
            modified: now,
            accessed: now,
            link_count: 1,
            device_id: None,
        }
    }
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    /// File name
    pub name: String,
    /// Inode number
    pub inode: InodeNumber,
    /// File type
    pub file_type: FileType,
}

/// File system error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    /// File or directory not found
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// File already exists
    AlreadyExists,
    /// Not a directory
    NotADirectory,
    /// Is a directory
    IsADirectory,
    /// Directory not empty
    DirectoryNotEmpty,
    /// Invalid argument
    InvalidArgument,
    /// No space left on device
    NoSpaceLeft,
    /// Read-only filesystem
    ReadOnly,
    /// I/O error
    IoError,
    /// Invalid file descriptor
    BadFileDescriptor,
    /// Operation not supported
    NotSupported,
    /// Cross-device link
    CrossDevice,
    /// Too many symbolic links
    TooManySymlinks,
    /// Filename too long
    NameTooLong,
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FsError::NotFound => write!(f, "No such file or directory"),
            FsError::PermissionDenied => write!(f, "Permission denied"),
            FsError::AlreadyExists => write!(f, "File exists"),
            FsError::NotADirectory => write!(f, "Not a directory"),
            FsError::IsADirectory => write!(f, "Is a directory"),
            FsError::DirectoryNotEmpty => write!(f, "Directory not empty"),
            FsError::InvalidArgument => write!(f, "Invalid argument"),
            FsError::NoSpaceLeft => write!(f, "No space left on device"),
            FsError::ReadOnly => write!(f, "Read-only file system"),
            FsError::IoError => write!(f, "Input/output error"),
            FsError::BadFileDescriptor => write!(f, "Bad file descriptor"),
            FsError::NotSupported => write!(f, "Operation not supported"),
            FsError::CrossDevice => write!(f, "Cross-device link"),
            FsError::TooManySymlinks => write!(f, "Too many levels of symbolic links"),
            FsError::NameTooLong => write!(f, "File name too long"),
        }
    }
}

/// File system result type
pub type FsResult<T> = Result<T, FsError>;

/// Open file flags
#[derive(Debug, Clone, Copy)]
pub struct OpenFlags {
    /// Read access
    pub read: bool,
    /// Write access
    pub write: bool,
    /// Create file if it doesn't exist
    pub create: bool,
    /// Truncate file to zero length
    pub truncate: bool,
    /// Append to end of file
    pub append: bool,
    /// Exclusive creation (fail if file exists)
    pub exclusive: bool,
}

impl OpenFlags {
    /// Create flags from POSIX-style flags
    pub fn from_posix(flags: u32) -> Self {
        const O_RDONLY: u32 = 0o0;
        const O_WRONLY: u32 = 0o1;
        const O_RDWR: u32 = 0o2;
        const O_CREAT: u32 = 0o100;
        const O_TRUNC: u32 = 0o1000;
        const O_APPEND: u32 = 0o2000;
        const O_EXCL: u32 = 0o200;

        let access_mode = flags & 0o3;
        Self {
            read: access_mode == O_RDONLY || access_mode == O_RDWR,
            write: access_mode == O_WRONLY || access_mode == O_RDWR,
            create: (flags & O_CREAT) != 0,
            truncate: (flags & O_TRUNC) != 0,
            append: (flags & O_APPEND) != 0,
            exclusive: (flags & O_EXCL) != 0,
        }
    }

    /// Read-only flags
    pub fn read_only() -> Self {
        Self {
            read: true,
            write: false,
            create: false,
            truncate: false,
            append: false,
            exclusive: false,
        }
    }

    /// Write-only flags
    pub fn write_only() -> Self {
        Self {
            read: false,
            write: true,
            create: false,
            truncate: false,
            append: false,
            exclusive: false,
        }
    }

    /// Read-write flags
    pub fn read_write() -> Self {
        Self {
            read: true,
            write: true,
            create: false,
            truncate: false,
            append: false,
            exclusive: false,
        }
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

// ============================================================================
// Syscall-compatible VFS Interface
// ============================================================================

bitflags! {
    /// POSIX-compatible open flags for syscall interface
    pub struct SyscallOpenFlags: u32 {
        /// Open for reading only
        const READ = 0o0;
        /// Open for writing only
        const WRITE = 0o1;
        /// Open for reading and writing
        const RDWR = 0o2;
        /// Create file if it doesn't exist
        const CREAT = 0o100;
        /// Fail if file exists (with CREAT)
        const EXCL = 0o200;
        /// Truncate file to zero length
        const TRUNC = 0o1000;
        /// Append mode
        const APPEND = 0o2000;
        /// Non-blocking mode
        const NONBLOCK = 0o4000;
        /// Synchronous I/O
        const SYNC = 0o10000;
    }
}

/// Inode handle for syscall interface
/// Represents an open file with methods for file operations
#[derive(Debug, Clone)]
pub struct Inode {
    /// Inode number
    inode_number: InodeNumber,
    /// File size
    size: u64,
    /// File mode (permissions and type)
    mode: u32,
    /// File type
    file_type: FileType,
    /// Reference to the filesystem
    mount_index: usize,
    /// File content cache (for RAM filesystem)
    content: Arc<RwLock<Vec<u8>>>,
}

impl Inode {
    /// Create a new inode handle
    pub fn new(
        inode_number: InodeNumber,
        size: u64,
        mode: u32,
        file_type: FileType,
        mount_index: usize,
    ) -> Self {
        Self {
            inode_number,
            size,
            mode,
            file_type,
            mount_index,
            content: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create inode with content
    pub fn with_content(
        inode_number: InodeNumber,
        size: u64,
        mode: u32,
        file_type: FileType,
        mount_index: usize,
        content: Vec<u8>,
    ) -> Self {
        Self {
            inode_number,
            size,
            mode,
            file_type,
            mount_index,
            content: Arc::new(RwLock::new(content)),
        }
    }

    /// Get the inode number
    pub fn inode_number(&self) -> InodeNumber {
        self.inode_number
    }

    /// Get the file size
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Get the file mode (permissions and type encoded as u32)
    pub fn mode(&self) -> u32 {
        self.mode
    }

    /// Get the file type
    pub fn file_type(&self) -> FileType {
        self.file_type
    }

    /// Read from the inode at a given offset
    pub fn read(&self, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let content = self.content.read();
        let content_len = content.len() as u64;

        if offset >= content_len {
            return Ok(0);
        }

        let start = offset as usize;
        let end = core::cmp::min(start + buffer.len(), content.len());
        let bytes_to_read = end - start;

        buffer[..bytes_to_read].copy_from_slice(&content[start..end]);
        Ok(bytes_to_read)
    }

    /// Write to the inode at a given offset
    pub fn write(&self, offset: u64, data: &[u8]) -> FsResult<usize> {
        let mut content = self.content.write();
        let required_len = (offset + data.len() as u64) as usize;

        if content.len() < required_len {
            content.resize(required_len, 0);
        }

        let start = offset as usize;
        let end = start + data.len();
        content[start..end].copy_from_slice(data);

        Ok(data.len())
    }

    /// Update the size after writes
    pub fn update_size(&mut self, new_size: u64) {
        self.size = new_size;
    }
}

/// Virtual File System interface for syscalls
/// Provides a simplified interface for process syscalls
pub struct VFS {
    /// Reference to the underlying VFS manager
    manager: &'static VfsManager,
}

impl VFS {
    /// Create a new VFS wrapper
    pub const fn new(manager: &'static VfsManager) -> Self {
        Self { manager }
    }

    /// Open a file and return an Inode handle
    ///
    /// # Arguments
    /// * `path` - Path to the file
    /// * `flags` - Open flags (SyscallOpenFlags or compatible)
    /// * `mode` - File creation mode (permissions)
    pub fn open(&self, path: &str, flags: SyscallOpenFlags, mode: u32) -> FsResult<Inode> {
        // Convert syscall flags to internal OpenFlags
        let internal_flags = OpenFlags {
            read: !flags.contains(SyscallOpenFlags::WRITE)
                || flags.contains(SyscallOpenFlags::RDWR),
            write: flags.contains(SyscallOpenFlags::WRITE)
                || flags.contains(SyscallOpenFlags::RDWR),
            create: flags.contains(SyscallOpenFlags::CREAT),
            truncate: flags.contains(SyscallOpenFlags::TRUNC),
            append: flags.contains(SyscallOpenFlags::APPEND),
            exclusive: flags.contains(SyscallOpenFlags::EXCL),
        };

        // Resolve the path
        let resolved_path = self.manager.resolve_path(path)?;
        let mount_index = self
            .manager
            .find_mount_point(&resolved_path)
            .ok_or(FsError::NotFound)?;

        let mount_points = self.manager.mount_points.read();
        let mount_point = &mount_points[mount_index];

        // Get relative path for the filesystem
        let relative_path = if mount_point.path == "/" {
            &resolved_path
        } else {
            resolved_path
                .strip_prefix(&mount_point.path)
                .unwrap_or(&resolved_path)
        };

        // Handle file creation
        let inode_number = if internal_flags.create {
            match mount_point.filesystem.open(relative_path, internal_flags) {
                Ok(inode) => inode,
                Err(FsError::NotFound) => {
                    // Create the file
                    let permissions = FilePermissions::from_octal(mode as u16);
                    mount_point.filesystem.create(relative_path, permissions)?
                }
                Err(e) => return Err(e),
            }
        } else {
            mount_point.filesystem.open(relative_path, internal_flags)?
        };

        // Get file metadata
        let metadata = mount_point.filesystem.metadata(inode_number)?;

        // Calculate mode value (file type + permissions)
        let file_type_bits = match metadata.file_type {
            FileType::Regular => 0o100000,
            FileType::Directory => 0o040000,
            FileType::SymbolicLink => 0o120000,
            FileType::CharacterDevice => 0o020000,
            FileType::BlockDevice => 0o060000,
            FileType::NamedPipe => 0o010000,
            FileType::Socket => 0o140000,
        };
        let mode_value = file_type_bits | (metadata.permissions.to_octal() as u32);

        // Read file content for RAM filesystem
        let mut content = Vec::new();
        if metadata.file_type == FileType::Regular && metadata.size > 0 {
            content.resize(metadata.size as usize, 0);
            let _ = mount_point.filesystem.read(inode_number, 0, &mut content);
        }

        Ok(Inode::with_content(
            inode_number,
            metadata.size,
            mode_value,
            metadata.file_type,
            mount_index,
            content,
        ))
    }

    /// Get file status without opening
    pub fn stat(&self, path: &str) -> FsResult<Inode> {
        self.open(path, SyscallOpenFlags::READ, 0)
    }

    /// Create a directory
    pub fn mkdir(&self, path: &str, mode: u32) -> FsResult<()> {
        let permissions = FilePermissions::from_octal(mode as u16);
        self.manager.mkdir(path, permissions)
    }

    /// Remove a file
    pub fn unlink(&self, path: &str) -> FsResult<()> {
        self.manager.unlink(path)
    }

    /// Remove a directory
    pub fn rmdir(&self, path: &str) -> FsResult<()> {
        self.manager.rmdir(path)
    }

    /// Get current working directory
    pub fn getcwd(&self) -> String {
        self.manager.getcwd()
    }

    /// Change current working directory
    pub fn chdir(&self, path: &str) -> FsResult<()> {
        self.manager.chdir(path)
    }
}

/// File system trait that all filesystems must implement
pub trait FileSystem: Send + Sync + fmt::Debug {
    /// Get filesystem type
    fn fs_type(&self) -> FileSystemType;

    /// Get filesystem statistics
    fn statfs(&self) -> FsResult<FileSystemStats>;

    /// Create a new file
    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber>;

    /// Open a file
    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber>;

    /// Read from a file
    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize>;

    /// Write to a file
    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize>;

    /// Get file metadata
    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata>;

    /// Set file metadata
    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()>;

    /// Create a directory
    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber>;

    /// Remove a directory
    fn rmdir(&self, path: &str) -> FsResult<()>;

    /// Remove a file
    fn unlink(&self, path: &str) -> FsResult<()>;

    /// Read directory entries
    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>>;

    /// Rename a file or directory
    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()>;

    /// Create a symbolic link
    fn symlink(&self, target: &str, link_path: &str) -> FsResult<()>;

    /// Read a symbolic link
    fn readlink(&self, path: &str) -> FsResult<String>;

    /// Sync filesystem data to storage
    fn sync(&self) -> FsResult<()>;
}

/// File system statistics
#[derive(Debug, Clone)]
pub struct FileSystemStats {
    /// Total blocks in filesystem
    pub total_blocks: u64,
    /// Free blocks available
    pub free_blocks: u64,
    /// Available blocks for unprivileged users
    pub available_blocks: u64,
    /// Total inodes
    pub total_inodes: u64,
    /// Free inodes
    pub free_inodes: u64,
    /// Block size in bytes
    pub block_size: u32,
    /// Maximum filename length
    pub max_filename_length: u32,
}

/// Mount point information
#[derive(Debug)]
pub struct MountPoint {
    /// Mount path
    pub path: String,
    /// Filesystem instance
    pub filesystem: Box<dyn FileSystem>,
    /// Mount flags
    pub flags: MountFlags,
}

/// Mount flags
#[derive(Debug, Clone, Copy)]
pub struct MountFlags {
    /// Read-only mount
    pub read_only: bool,
    /// No execution of binaries
    pub no_exec: bool,
    /// No device files
    pub no_dev: bool,
    /// No setuid/setgid
    pub no_suid: bool,
}

impl Default for MountFlags {
    fn default() -> Self {
        Self {
            read_only: false,
            no_exec: false,
            no_dev: false,
            no_suid: false,
        }
    }
}

/// Open file descriptor
#[derive(Debug)]
pub struct OpenFile {
    /// Inode number
    pub inode: InodeNumber,
    /// Mount point index
    pub mount_index: usize,
    /// Current file position
    pub position: u64,
    /// Open flags
    pub flags: OpenFlags,
    /// Reference count
    pub ref_count: u32,
}

/// Virtual File System manager
pub struct VfsManager {
    /// Mount points
    mount_points: RwLock<Vec<MountPoint>>,
    /// Open file descriptors
    open_files: RwLock<BTreeMap<FileDescriptor, OpenFile>>,
    /// Next file descriptor to allocate
    next_fd: Mutex<FileDescriptor>,
    /// Current working directory
    current_dir: RwLock<String>,
}

impl VfsManager {
    /// Create a new VFS manager
    pub fn new() -> Self {
        Self {
            mount_points: RwLock::new(Vec::new()),
            open_files: RwLock::new(BTreeMap::new()),
            next_fd: Mutex::new(3), // Start after stdin, stdout, stderr
            current_dir: RwLock::new("/".to_string()),
        }
    }

    /// Mount a filesystem
    pub fn mount(
        &self,
        path: &str,
        filesystem: Box<dyn FileSystem>,
        flags: MountFlags,
    ) -> FsResult<()> {
        let mut mount_points = self.mount_points.write();

        // Check if mount point already exists
        if mount_points.iter().any(|mp| mp.path == path) {
            return Err(FsError::AlreadyExists);
        }

        mount_points.push(MountPoint {
            path: path.to_string(),
            filesystem,
            flags,
        });

        // Sort mount points by path length (longest first) for proper resolution
        mount_points.sort_by(|a, b| b.path.len().cmp(&a.path.len()));
        Ok(())
    }

    /// Unmount a filesystem
    pub fn unmount(&self, path: &str) -> FsResult<()> {
        let mut mount_points = self.mount_points.write();

        if let Some(pos) = mount_points.iter().position(|mp| mp.path == path) {
            mount_points.remove(pos);
            Ok(())
        } else {
            Err(FsError::NotFound)
        }
    }

    /// Find the mount point for a given path
    fn find_mount_point(&self, path: &str) -> Option<usize> {
        let mount_points = self.mount_points.read();
        mount_points
            .iter()
            .position(|mp| path.starts_with(&mp.path))
    }

    /// Open a file
    pub fn open(&self, path: &str, flags: OpenFlags) -> FsResult<FileDescriptor> {
        let resolved_path = self.resolve_path(path)?;
        let mount_index = self
            .find_mount_point(&resolved_path)
            .ok_or(FsError::NotFound)?;

        let mount_points = self.mount_points.read();
        let mount_point = &mount_points[mount_index];

        // Remove mount point prefix from path
        let relative_path = if mount_point.path == "/" {
            &resolved_path
        } else {
            resolved_path
                .strip_prefix(&mount_point.path)
                .unwrap_or(&resolved_path)
        };

        let inode = mount_point.filesystem.open(relative_path, flags)?;

        // Allocate file descriptor
        let fd = {
            let mut next_fd = self.next_fd.lock();
            let fd = *next_fd;
            *next_fd += 1;
            fd
        };

        let open_file = OpenFile {
            inode,
            mount_index,
            position: 0,
            flags,
            ref_count: 1,
        };

        self.open_files.write().insert(fd, open_file);
        Ok(fd)
    }

    /// Close a file descriptor
    pub fn close(&self, fd: FileDescriptor) -> FsResult<()> {
        let mut open_files = self.open_files.write();
        if open_files.remove(&fd).is_some() {
            Ok(())
        } else {
            Err(FsError::BadFileDescriptor)
        }
    }

    /// Read from a file descriptor
    pub fn read(&self, fd: FileDescriptor, buffer: &mut [u8]) -> FsResult<usize> {
        let mut open_files = self.open_files.write();
        let open_file = open_files.get_mut(&fd).ok_or(FsError::BadFileDescriptor)?;

        if !open_file.flags.read {
            return Err(FsError::PermissionDenied);
        }

        let mount_points = self.mount_points.read();
        let mount_point = &mount_points[open_file.mount_index];

        let bytes_read =
            mount_point
                .filesystem
                .read(open_file.inode, open_file.position, buffer)?;
        open_file.position += bytes_read as u64;

        Ok(bytes_read)
    }

    /// Write to a file descriptor
    pub fn write(&self, fd: FileDescriptor, buffer: &[u8]) -> FsResult<usize> {
        let mut open_files = self.open_files.write();
        let open_file = open_files.get_mut(&fd).ok_or(FsError::BadFileDescriptor)?;

        if !open_file.flags.write {
            return Err(FsError::PermissionDenied);
        }

        let mount_points = self.mount_points.read();
        let mount_point = &mount_points[open_file.mount_index];

        let position = if open_file.flags.append {
            // For append mode, always write at the end
            let metadata = mount_point.filesystem.metadata(open_file.inode)?;
            metadata.size
        } else {
            open_file.position
        };

        let bytes_written = mount_point
            .filesystem
            .write(open_file.inode, position, buffer)?;

        if !open_file.flags.append {
            open_file.position += bytes_written as u64;
        }

        Ok(bytes_written)
    }

    /// Get file size by file descriptor
    pub fn fstat_size(&self, fd: FileDescriptor) -> FsResult<u64> {
        let open_files = self.open_files.read();
        let open_file = open_files.get(&fd).ok_or(FsError::BadFileDescriptor)?;

        let mount_points = self.mount_points.read();
        let mount_point = &mount_points[open_file.mount_index];
        let metadata = mount_point.filesystem.metadata(open_file.inode)?;
        Ok(metadata.size)
    }

    /// Seek in a file
    pub fn seek(&self, fd: FileDescriptor, pos: SeekFrom) -> FsResult<u64> {
        let mut open_files = self.open_files.write();
        let open_file = open_files.get_mut(&fd).ok_or(FsError::BadFileDescriptor)?;

        let mount_points = self.mount_points.read();
        let mount_point = &mount_points[open_file.mount_index];
        let metadata = mount_point.filesystem.metadata(open_file.inode)?;

        let new_position = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::Current(offset) => {
                if offset >= 0 {
                    open_file.position + offset as u64
                } else {
                    open_file.position.saturating_sub((-offset) as u64)
                }
            }
            SeekFrom::End(offset) => {
                if offset >= 0 {
                    metadata.size + offset as u64
                } else {
                    metadata.size.saturating_sub((-offset) as u64)
                }
            }
        };

        open_file.position = new_position;
        Ok(new_position)
    }

    /// Get file metadata
    pub fn stat(&self, path: &str) -> FsResult<FileMetadata> {
        let resolved_path = self.resolve_path(path)?;
        let mount_index = self
            .find_mount_point(&resolved_path)
            .ok_or(FsError::NotFound)?;

        let mount_points = self.mount_points.read();
        let mount_point = &mount_points[mount_index];

        let relative_path = if mount_point.path == "/" {
            &resolved_path
        } else {
            resolved_path
                .strip_prefix(&mount_point.path)
                .unwrap_or(&resolved_path)
        };

        let inode = mount_point
            .filesystem
            .open(relative_path, OpenFlags::read_only())?;
        mount_point.filesystem.metadata(inode)
    }

    /// Create a directory
    pub fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<()> {
        let resolved_path = self.resolve_path(path)?;
        let mount_index = self
            .find_mount_point(&resolved_path)
            .ok_or(FsError::NotFound)?;

        let mount_points = self.mount_points.read();
        let mount_point = &mount_points[mount_index];

        if mount_point.flags.read_only {
            return Err(FsError::ReadOnly);
        }

        let relative_path = if mount_point.path == "/" {
            &resolved_path
        } else {
            resolved_path
                .strip_prefix(&mount_point.path)
                .unwrap_or(&resolved_path)
        };

        mount_point.filesystem.mkdir(relative_path, permissions)?;
        Ok(())
    }

    /// Remove a directory
    pub fn rmdir(&self, path: &str) -> FsResult<()> {
        let resolved_path = self.resolve_path(path)?;
        let mount_index = self
            .find_mount_point(&resolved_path)
            .ok_or(FsError::NotFound)?;

        let mount_points = self.mount_points.read();
        let mount_point = &mount_points[mount_index];

        if mount_point.flags.read_only {
            return Err(FsError::ReadOnly);
        }

        let relative_path = if mount_point.path == "/" {
            &resolved_path
        } else {
            resolved_path
                .strip_prefix(&mount_point.path)
                .unwrap_or(&resolved_path)
        };

        mount_point.filesystem.rmdir(relative_path)
    }

    /// Remove a file
    pub fn unlink(&self, path: &str) -> FsResult<()> {
        let resolved_path = self.resolve_path(path)?;
        let mount_index = self
            .find_mount_point(&resolved_path)
            .ok_or(FsError::NotFound)?;

        let mount_points = self.mount_points.read();
        let mount_point = &mount_points[mount_index];

        if mount_point.flags.read_only {
            return Err(FsError::ReadOnly);
        }

        let relative_path = if mount_point.path == "/" {
            &resolved_path
        } else {
            resolved_path
                .strip_prefix(&mount_point.path)
                .unwrap_or(&resolved_path)
        };

        mount_point.filesystem.unlink(relative_path)
    }

    /// Change current working directory
    pub fn chdir(&self, path: &str) -> FsResult<()> {
        let resolved_path = self.resolve_path(path)?;

        // Verify the directory exists
        let metadata = self.stat(&resolved_path)?;
        if metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        *self.current_dir.write() = resolved_path;
        Ok(())
    }

    /// Get current working directory
    pub fn getcwd(&self) -> String {
        self.current_dir.read().clone()
    }

    /// Resolve a path (handle relative paths, . and ..)
    fn resolve_path(&self, path: &str) -> FsResult<String> {
        if path.is_empty() {
            return Err(FsError::InvalidArgument);
        }

        let mut components = Vec::new();

        // Start with current directory if path is relative
        if !path.starts_with('/') {
            let cwd = self.current_dir.read();
            for component in cwd.split('/').filter(|c| !c.is_empty()) {
                components.push(component.to_string());
            }
        }

        // Process path components
        for component in path.split('/').filter(|c| !c.is_empty()) {
            match component {
                "." => continue, // Current directory, ignore
                ".." => {
                    components.pop(); // Parent directory
                }
                _ => components.push(component.to_string()),
            }
        }

        // Build final path
        if components.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", components.join("/")))
        }
    }

    /// List mount points
    pub fn list_mounts(&self) -> Vec<(String, FileSystemType)> {
        let mount_points = self.mount_points.read();
        mount_points
            .iter()
            .map(|mp| (mp.path.clone(), mp.filesystem.fs_type()))
            .collect()
    }
}

lazy_static! {
    /// Global VFS manager instance
    static ref VFS_MANAGER: VfsManager = VfsManager::new();

    /// Static VFS instance for syscalls
    static ref SYSCALL_VFS: VFS = VFS::new(&VFS_MANAGER);
}

/// Get the global VFS instance for syscall interface
pub fn get_vfs() -> &'static VFS {
    &SYSCALL_VFS
}

/// Initialize the VFS subsystem
pub fn init() -> FsResult<()> {
    // Initialize buffer cache
    buffer::init_buffer_cache();

    // Try mounting a real filesystem from the default block device, if one exists.
    let storage_device = crate::drivers::storage::get_default_device();
    let root_mounted = if let Some(device_id) = storage_device {
        if let Ok(ext4_fs) = ext4::Ext4FileSystem::new(device_id) {
            // Mount EXT4 filesystem as root
            let ext4_box = Box::new(ext4_fs);
            VFS_MANAGER
                .mount("/", ext4_box, MountFlags::default())
                .is_ok()
        } else if let Ok(fat32_fs) = fat32::Fat32FileSystem::new(device_id) {
            // Mount FAT32 filesystem as root
            let fat32_box = Box::new(fat32_fs);
            VFS_MANAGER
                .mount("/", fat32_box, MountFlags::default())
                .is_ok()
        } else {
            false
        }
    } else {
        false
    };

    // Fall back to RAM filesystem if no real filesystem found
    if !root_mounted {
        let root_fs = Box::new(ramfs::RamFs::new());
        VFS_MANAGER.mount("/", root_fs, MountFlags::default())?;
    }

// Mount devfs at /dev — leak the box to get a stable instance for
    // dynamic device registration, then mount a thin wrapper around it.
    let dev_fs = Box::new(devfs::DevFs::new());
    let dev_fs_ref: &'static devfs::DevFs = Box::leak(dev_fs);
    devfs::register_devfs(dev_fs_ref);
    VFS_MANAGER.mount(
        "/dev",
        Box::new(devfs::DevFsMount(dev_fs_ref)),
        MountFlags::default(),
    )?;

        // Create standard directories (only if using RAM filesystem)
    if !root_mounted {
        VFS_MANAGER.mkdir("/tmp", FilePermissions::from_octal(0o755))?;
        VFS_MANAGER.mkdir("/proc", FilePermissions::from_octal(0o755))?;
        VFS_MANAGER.mkdir("/sys", FilePermissions::from_octal(0o755))?;
        VFS_MANAGER.mkdir("/home", FilePermissions::from_octal(0o755))?;
        VFS_MANAGER.mkdir("/usr", FilePermissions::from_octal(0o755))?;
        VFS_MANAGER.mkdir("/var", FilePermissions::from_octal(0o755))?;
    }

    Ok(())
}

/// Mount a filesystem from a storage device
pub fn mount_filesystem(
    device_id: u32,
    mount_point: &str,
    fs_type: Option<FileSystemType>,
) -> FsResult<()> {
    let filesystem: Box<dyn FileSystem> = match fs_type {
        Some(FileSystemType::Ext2) => Box::new(ext4::Ext4FileSystem::new(device_id)?),
        Some(FileSystemType::Fat32) => Box::new(fat32::Fat32FileSystem::new(device_id)?),
        _ => {
            // Auto-detect filesystem type
            if let Ok(ext4_fs) = ext4::Ext4FileSystem::new(device_id) {
                Box::new(ext4_fs)
            } else if let Ok(fat32_fs) = fat32::Fat32FileSystem::new(device_id) {
                Box::new(fat32_fs)
            } else {
                return Err(FsError::NotSupported);
            }
        }
    };

    VFS_MANAGER.mount(mount_point, filesystem, MountFlags::default())
}

/// Unmount a filesystem
pub fn unmount_filesystem(mount_point: &str) -> FsResult<()> {
    // Flush all buffers for the filesystem before unmounting
    let _ = buffer::flush_all_buffers();
    VFS_MANAGER.unmount(mount_point)
}

/// Get the global VFS manager
pub fn vfs() -> &'static VfsManager {
    &VFS_MANAGER
}

/// Get current time in milliseconds
fn get_current_time() -> u64 {
    // Use system time for filesystem timestamps
    crate::time::get_system_time_ms()
}
