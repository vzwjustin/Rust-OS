//! VFS Usage Examples
//!
//! This module provides examples of how to use the VFS API.

#![allow(dead_code)]

use super::*;
use alloc::format;

/// Example 1: Basic file operations
pub fn example_basic_file_ops() -> VfsResult<()> {
    // Initialize VFS
    init()?;

    // Create and write to a file
    let fd = vfs_open("/hello.txt", OpenFlags::RDWR | OpenFlags::CREAT, 0o644)?;
    let message = b"Hello, RustOS VFS!";
    vfs_write(fd, message)?;
    vfs_close(fd)?;

    // Read the file back
    let fd = vfs_open("/hello.txt", OpenFlags::RDONLY, 0)?;
    let mut buffer = [0u8; 1024];
    let bytes_read = vfs_read(fd, &mut buffer)?;
    vfs_close(fd)?;

    // Verify content
    assert_eq!(&buffer[..bytes_read], message);

    Ok(())
}

/// Example 2: Directory operations
pub fn example_directory_ops() -> VfsResult<()> {
    init()?;

    // Create directory structure
    vfs_mkdir("/tmp", 0o755)?;
    vfs_mkdir("/tmp/test", 0o755)?;

    // Create files in directory
    let fd = vfs_open(
        "/tmp/test/file1.txt",
        OpenFlags::RDWR | OpenFlags::CREAT,
        0o644,
    )?;
    vfs_write(fd, b"File 1 content")?;
    vfs_close(fd)?;

    let fd = vfs_open(
        "/tmp/test/file2.txt",
        OpenFlags::RDWR | OpenFlags::CREAT,
        0o644,
    )?;
    vfs_write(fd, b"File 2 content")?;
    vfs_close(fd)?;

    // List directory contents
    let entries = vfs_readdir("/tmp/test")?;
    for entry in entries {
        // Process each entry
        let stat = vfs_stat(&format!("/tmp/test/{}", entry.name))?;
        // Use stat information...
    }

    // Cleanup
    vfs_unlink("/tmp/test/file1.txt")?;
    vfs_unlink("/tmp/test/file2.txt")?;
    vfs_rmdir("/tmp/test")?;
    vfs_rmdir("/tmp")?;

    Ok(())
}

/// Example 3: Seek operations
pub fn example_seek_ops() -> VfsResult<()> {
    init()?;

    // Create a file with known content
    let fd = vfs_open("/seektest.txt", OpenFlags::RDWR | OpenFlags::CREAT, 0o644)?;
    vfs_write(fd, b"0123456789ABCDEF")?;

    // Seek to middle
    vfs_seek(fd, SeekFrom::Start(5))?;
    let mut buffer = [0u8; 5];
    vfs_read(fd, &mut buffer)?;
    assert_eq!(&buffer, b"56789");

    // Seek from current position
    vfs_seek(fd, SeekFrom::Current(-3))?;
    vfs_read(fd, &mut buffer[..2])?;
    assert_eq!(&buffer[..2], b"78");

    // Seek from end
    vfs_seek(fd, SeekFrom::End(-4))?;
    vfs_read(fd, &mut buffer[..4])?;
    assert_eq!(&buffer[..4], b"CDEF");

    vfs_close(fd)?;
    vfs_unlink("/seektest.txt")?;

    Ok(())
}

/// Example 4: File metadata
pub fn example_file_metadata() -> VfsResult<()> {
    init()?;

    // Create a file
    let fd = vfs_open("/metadata.txt", OpenFlags::RDWR | OpenFlags::CREAT, 0o644)?;
    vfs_write(fd, b"Some data for metadata testing")?;

    // Get metadata via file descriptor
    let stat = vfs_fstat(fd)?;
    assert_eq!(stat.inode_type, InodeType::File);
    assert_eq!(stat.size, 30);
    assert_eq!(stat.mode & 0o777, 0o644);

    vfs_close(fd)?;

    // Get metadata via path
    let stat = vfs_stat("/metadata.txt")?;
    assert_eq!(stat.size, 30);

    vfs_unlink("/metadata.txt")?;

    Ok(())
}

/// Example 5: Append mode
pub fn example_append_mode() -> VfsResult<()> {
    init()?;

    // Create initial file
    let fd = vfs_open("/append.txt", OpenFlags::RDWR | OpenFlags::CREAT, 0o644)?;
    vfs_write(fd, b"Line 1\n")?;
    vfs_close(fd)?;

    // Open in append mode and add more content
    let fd = vfs_open("/append.txt", OpenFlags::WRONLY | OpenFlags::APPEND, 0)?;
    vfs_write(fd, b"Line 2\n")?;
    vfs_write(fd, b"Line 3\n")?;
    vfs_close(fd)?;

    // Verify all content is present
    let fd = vfs_open("/append.txt", OpenFlags::RDONLY, 0)?;
    let mut buffer = [0u8; 1024];
    let bytes_read = vfs_read(fd, &mut buffer)?;
    vfs_close(fd)?;

    assert_eq!(&buffer[..bytes_read], b"Line 1\nLine 2\nLine 3\n");

    vfs_unlink("/append.txt")?;

    Ok(())
}

/// Example 6: Truncate operation
pub fn example_truncate() -> VfsResult<()> {
    init()?;

    // Create file with content
    let fd = vfs_open("/trunc.txt", OpenFlags::RDWR | OpenFlags::CREAT, 0o644)?;
    vfs_write(fd, b"This is a long text that will be truncated")?;
    let stat = vfs_fstat(fd)?;
    assert_eq!(stat.size, 43);
    vfs_close(fd)?;

    // Open with truncate flag
    let fd = vfs_open("/trunc.txt", OpenFlags::RDWR | OpenFlags::TRUNC, 0)?;
    let stat = vfs_fstat(fd)?;
    assert_eq!(stat.size, 0);

    vfs_write(fd, b"New content")?;
    let stat = vfs_fstat(fd)?;
    assert_eq!(stat.size, 11);

    vfs_close(fd)?;
    vfs_unlink("/trunc.txt")?;

    Ok(())
}

/// Example 7: Error handling
pub fn example_error_handling() -> VfsResult<()> {
    init()?;

    // Try to open non-existent file without CREATE flag
    match vfs_open("/nonexistent.txt", OpenFlags::RDONLY, 0) {
        Err(VfsError::NotFound) => {
            // Expected error
        }
        _ => panic!("Expected NotFound error"),
    }

    // Try to write to read-only file
    let fd = vfs_open("/readonly.txt", OpenFlags::RDWR | OpenFlags::CREAT, 0o644)?;
    vfs_write(fd, b"content")?;
    vfs_close(fd)?;

    let fd = vfs_open("/readonly.txt", OpenFlags::RDONLY, 0)?;
    match vfs_write(fd, b"try to write") {
        Err(VfsError::PermissionDenied) => {
            // Expected error
        }
        _ => panic!("Expected PermissionDenied error"),
    }
    vfs_close(fd)?;

    // Try to use bad file descriptor
    match vfs_read(999, &mut [0u8; 10]) {
        Err(VfsError::BadFileDescriptor) => {
            // Expected error
        }
        _ => panic!("Expected BadFileDescriptor error"),
    }

    vfs_unlink("/readonly.txt")?;

    Ok(())
}

/// Example 8: Multiple file descriptors
pub fn example_multiple_fds() -> VfsResult<()> {
    init()?;

    // Create a file
    let fd = vfs_open("/multi.txt", OpenFlags::RDWR | OpenFlags::CREAT, 0o644)?;
    vfs_write(fd, b"0123456789")?;
    vfs_close(fd)?;

    // Open multiple descriptors to the same file
    let fd1 = vfs_open("/multi.txt", OpenFlags::RDONLY, 0)?;
    let fd2 = vfs_open("/multi.txt", OpenFlags::RDONLY, 0)?;

    // Each descriptor has independent position
    vfs_seek(fd1, SeekFrom::Start(0))?;
    vfs_seek(fd2, SeekFrom::Start(5))?;

    let mut buf1 = [0u8; 5];
    let mut buf2 = [0u8; 5];

    vfs_read(fd1, &mut buf1)?;
    vfs_read(fd2, &mut buf2)?;

    assert_eq!(&buf1, b"01234");
    assert_eq!(&buf2, b"56789");

    vfs_close(fd1)?;
    vfs_close(fd2)?;
    vfs_unlink("/multi.txt")?;

    Ok(())
}

/// Example 9: Dup operations
pub fn example_dup() -> VfsResult<()> {
    init()?;

    // Create a file
    let fd = vfs_open("/dup.txt", OpenFlags::RDWR | OpenFlags::CREAT, 0o644)?;
    vfs_write(fd, b"Test content for dup")?;

    // Duplicate the file descriptor
    let vfs = get_vfs();
    let fd_dup = vfs.dup(fd)?;

    // Both descriptors refer to the same file
    vfs_seek(fd, SeekFrom::Start(0))?;

    let mut buf1 = [0u8; 4];
    let mut buf2 = [0u8; 4];

    vfs_read(fd, &mut buf1)?;
    vfs_read(fd_dup, &mut buf2)?; // Continues from where fd left off

    // Note: Position is shared between duplicated descriptors
    assert_eq!(&buf1, b"Test");

    vfs_close(fd)?;
    vfs_close(fd_dup)?;
    vfs_unlink("/dup.txt")?;

    Ok(())
}

/// Example 10: Integration with syscalls
pub fn example_syscall_integration() {
    // This demonstrates how VFS operations can be used to implement syscalls

    // sys_open implementation
    fn sys_open_impl(path: *const u8, flags: i32, mode: u32) -> i32 {
        // Convert path from C string
        let path_str = "/example.txt"; // In real code, convert from pointer

        // Convert flags
        let open_flags = OpenFlags::new(flags as u32);

        // Call VFS
        match vfs_open(path_str, open_flags.bits(), mode) {
            Ok(fd) => fd,
            Err(_) => -1, // Return error code
        }
    }

    // sys_read implementation
    fn sys_read_impl(fd: i32, buf: *mut u8, count: usize) -> isize {
        // Create buffer slice (unsafe)
        let buffer = unsafe { core::slice::from_raw_parts_mut(buf, count) };

        // Call VFS
        match vfs_read(fd, buffer) {
            Ok(n) => n as isize,
            Err(_) => -1,
        }
    }

    // sys_write implementation
    fn sys_write_impl(fd: i32, buf: *const u8, count: usize) -> isize {
        // Create buffer slice (unsafe)
        let buffer = unsafe { core::slice::from_raw_parts(buf, count) };

        // Call VFS
        match vfs_write(fd, buffer) {
            Ok(n) => n as isize,
            Err(_) => -1,
        }
    }

    // sys_close implementation
    fn sys_close_impl(fd: i32) -> i32 {
        match vfs_close(fd) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }
}
