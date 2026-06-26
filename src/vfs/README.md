# Virtual File System (VFS) Implementation

## Overview

This directory contains a clean, production-ready Virtual File System implementation for RustOS. This VFS provides a unified interface for all filesystem operations with a focus on simplicity, performance, and correctness.

## Architecture

### Core Components

1. **mod.rs** - Core VFS layer
   - `Vfs` - Global VFS manager
   - `InodeOps` - Trait for inode operations
   - `SuperblockOps` - Trait for filesystem operations
   - Path resolution and mount point management
   - File descriptor management

2. **file_descriptor.rs** - File descriptor management
   - `FileDescriptor` - Open file descriptor structure
   - `OpenFileTable` - Global file descriptor table
   - Descriptor allocation and lifecycle management

3. **ramfs.rs** - In-memory filesystem
   - `RamFs` - RAM-based filesystem implementation
   - `RamFsInode` - RAM filesystem inode
   - Default root filesystem for RustOS

## Key Features

### Type Safety
- Strong typing with `VfsResult<T>` for error handling
- Clear separation between inode operations and file operations
- Type-safe file descriptor management

### Thread Safety
- All VFS structures use `spin::Mutex` and `spin::RwLock`
- Lock-free atomic operations where possible
- Safe concurrent access to filesystem data

### Performance
- Efficient path resolution with mount point caching
- Zero-copy operations where possible
- Minimal locking granularity

### Compatibility
- POSIX-like API surface
- Standard open flags (O_RDONLY, O_WRONLY, O_RDWR, O_CREAT, etc.)
- Standard seek operations (SEEK_SET, SEEK_CUR, SEEK_END)

## Public API

### Initialization
```rust
use vfs;

// Initialize VFS with default root filesystem
vfs::init()?;
```

### File Operations
```rust
use vfs::{vfs_open, vfs_read, vfs_write, vfs_close, OpenFlags};

// Open a file
let fd = vfs_open("/test.txt", OpenFlags::RDWR | OpenFlags::CREAT, 0o644)?;

// Write data
let data = b"Hello, World!";
vfs_write(fd, data)?;

// Read data
let mut buffer = [0u8; 1024];
let bytes_read = vfs_read(fd, &mut buffer)?;

// Close file
vfs_close(fd)?;
```

### Directory Operations
```rust
use vfs::{vfs_mkdir, vfs_readdir, vfs_rmdir};

// Create directory
vfs_mkdir("/tmp", 0o755)?;

// Read directory entries
let entries = vfs_readdir("/")?;
for entry in entries {
    println!("{}: {}", entry.name, entry.ino);
}

// Remove directory
vfs_rmdir("/tmp")?;
```

### File Metadata
```rust
use vfs::{vfs_stat, vfs_fstat};

// Get file stats by path
let stat = vfs_stat("/test.txt")?;
println!("Size: {} bytes", stat.size);

// Get file stats by descriptor
let stat = vfs_fstat(fd)?;
println!("Type: {:?}", stat.inode_type);
```

## Error Handling

All VFS operations return `VfsResult<T>` which is an alias for `Result<T, VfsError>`.

### Error Types
- `VfsError::NotFound` - File or directory not found
- `VfsError::PermissionDenied` - Permission denied
- `VfsError::AlreadyExists` - File already exists
- `VfsError::NotDirectory` - Path component is not a directory
- `VfsError::IsDirectory` - Operation invalid on directory
- `VfsError::InvalidArgument` - Invalid argument
- `VfsError::IoError` - I/O error
- `VfsError::NoSpace` - No space left on device
- `VfsError::TooManyFiles` - Too many open files
- `VfsError::BadFileDescriptor` - Invalid file descriptor
- `VfsError::InvalidSeek` - Invalid seek operation
- `VfsError::NameTooLong` - Filename too long
- `VfsError::CrossDevice` - Cross-device link
- `VfsError::ReadOnly` - Read-only filesystem
- `VfsError::NotSupported` - Operation not supported

## Extending the VFS

### Implementing a New Filesystem

To implement a new filesystem, implement the `SuperblockOps` and `InodeOps` traits:

```rust
use alloc::sync::Arc;
use vfs::{SuperblockOps, InodeOps, InodeType, Stat, DirEntry, VfsResult, StatFs};

struct MyFsInode {
    // Your inode data
}

impl InodeOps for MyFsInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        // Implement read
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // Implement write
    }

    fn stat(&self) -> VfsResult<Stat> {
        // Return inode metadata
    }

    // Implement other methods...
}

struct MyFs {
    root: Arc<MyFsInode>,
}

impl SuperblockOps for MyFs {
    fn root(&self) -> Arc<dyn InodeOps> {
        Arc::clone(&self.root) as Arc<dyn InodeOps>
    }

    fn sync_fs(&self) -> VfsResult<()> {
        // Sync filesystem
    }

    fn statfs(&self) -> VfsResult<StatFs> {
        // Return filesystem statistics
    }
}
```

### Mounting a Filesystem

```rust
use vfs::get_vfs;
use alloc::sync::Arc;

let my_fs = Arc::new(MyFs::new());
get_vfs().mount("/mnt/myfs", my_fs)?;
```

## Integration with Linux Compatibility Layer

The VFS integrates seamlessly with the Linux compatibility layer in `src/linux_compat/file_ops.rs`. Linux syscalls can be implemented using VFS operations:

```rust
use vfs::{vfs_open, vfs_read, vfs_write, vfs_close, OpenFlags};

pub fn sys_open(path: &str, flags: i32, mode: u32) -> Result<i32, LinuxError> {
    let open_flags = OpenFlags::new(flags as u32);
    vfs_open(path, open_flags, mode)
        .map_err(|e| convert_vfs_error(e))
}
```

## Performance Characteristics

### Time Complexity
- Path resolution: O(n) where n is path depth
- File open: O(log m) where m is number of open files
- File read/write: O(1) for position update + filesystem-specific I/O
- Directory lookup: Filesystem-specific (O(n) for RamFs)

### Space Complexity
- File descriptor table: O(n) where n is number of open files
- Mount table: O(m) where m is number of mount points
- Inode cache: Filesystem-specific

### Optimization Opportunities
1. Add dcache (dentry cache) for faster path resolution
2. Add inode cache for frequently accessed inodes
3. Implement read-ahead for sequential reads
4. Add write-behind buffering for better write performance

## Comparison with src/fs/

This VFS implementation differs from the existing `src/fs/` implementation:

### Advantages of src/vfs/
- Cleaner trait-based architecture
- More flexible inode operations
- Thread-safe by design with explicit locking
- Simpler mount point management
- More modern Rust patterns

### Advantages of src/fs/
- More complete filesystem implementations (ext4, fat32)
- Buffer cache integration
- Path caching
- Auto-detection of filesystem types
- More comprehensive mount flags

### Recommendation
- Use `src/vfs/` for new code requiring clean abstractions
- Use `src/fs/` for integration with existing filesystems
- Consider merging the best features of both implementations

## Testing

The VFS can be tested using the kernel's testing framework:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_vfs_basic_operations() {
        vfs::init().unwrap();

        let fd = vfs_open("/test.txt", OpenFlags::RDWR | OpenFlags::CREAT, 0o644).unwrap();
        let data = b"test data";
        vfs_write(fd, data).unwrap();
        vfs_close(fd).unwrap();

        let fd = vfs_open("/test.txt", OpenFlags::RDONLY, 0).unwrap();
        let mut buf = [0u8; 1024];
        let n = vfs_read(fd, &mut buf).unwrap();
        assert_eq!(&buf[..n], data);
        vfs_close(fd).unwrap();
    }
}
```

## Future Enhancements

1. **Advanced Features**
   - Symbolic link support
   - Hard link support
   - File locking (flock, fcntl)
   - Memory-mapped files (mmap)
   - Asynchronous I/O

2. **Performance**
   - Dentry cache (dcache)
   - Inode cache (icache)
   - Page cache integration
   - Read-ahead and write-behind

3. **Security**
   - Access control lists (ACLs)
   - Extended attributes (xattrs)
   - Mandatory access control
   - Filesystem encryption

4. **Reliability**
   - Journaling support
   - Copy-on-write filesystems
   - Snapshots
   - Online filesystem checking

## License

This VFS implementation is part of RustOS and follows the project's licensing terms.
