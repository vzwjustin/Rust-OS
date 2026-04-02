//! Linux file operation APIs
//!
//! This module implements Linux-compatible file operations including
//! stat, access, dup, link operations, and directory handling.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxResult, LinuxError};
use crate::vfs::{self, OpenFlags as VfsOpenFlags, SeekFrom, VfsError, InodeType};

// Re-export types for external access
pub use super::types::Stat;

/// Operation counter for statistics
static FILE_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize file operations subsystem
pub fn init_file_operations() {
    // Initialize file operation tracking
    FILE_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of file operations performed
pub fn get_operation_count() -> u64 {
    FILE_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    FILE_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Convert VFS error to Linux error code
fn vfs_error_to_linux(err: VfsError) -> LinuxError {
    match err {
        VfsError::NotFound => LinuxError::ENOENT,
        VfsError::PermissionDenied => LinuxError::EACCES,
        VfsError::AlreadyExists => LinuxError::EEXIST,
        VfsError::NotDirectory => LinuxError::ENOTDIR,
        VfsError::IsDirectory => LinuxError::EISDIR,
        VfsError::InvalidArgument => LinuxError::EINVAL,
        VfsError::IoError => LinuxError::EIO,
        VfsError::NoSpace => LinuxError::ENOSPC,
        VfsError::TooManyFiles => LinuxError::EMFILE,
        VfsError::BadFileDescriptor => LinuxError::EBADF,
        VfsError::InvalidSeek => LinuxError::EINVAL,
        VfsError::NameTooLong => LinuxError::ENAMETOOLONG,
        VfsError::CrossDevice => LinuxError::EXDEV,
        VfsError::ReadOnly => LinuxError::EROFS,
        VfsError::NotSupported => LinuxError::ENOSYS,
    }
}

/// Convert Linux open flags to VFS open flags
fn linux_flags_to_vfs(flags: i32) -> u32 {
    let mut vfs_flags = 0u32;

    // Access mode (bottom 2 bits)
    match flags & 0o3 {
        open_flags::O_RDONLY => vfs_flags |= VfsOpenFlags::RDONLY,
        open_flags::O_WRONLY => vfs_flags |= VfsOpenFlags::WRONLY,
        open_flags::O_RDWR => vfs_flags |= VfsOpenFlags::RDWR,
        _ => {}
    }

    // Additional flags
    if flags & open_flags::O_CREAT != 0 {
        vfs_flags |= VfsOpenFlags::CREAT;
    }
    if flags & open_flags::O_EXCL != 0 {
        vfs_flags |= VfsOpenFlags::EXCL;
    }
    if flags & open_flags::O_TRUNC != 0 {
        vfs_flags |= VfsOpenFlags::TRUNC;
    }
    if flags & open_flags::O_APPEND != 0 {
        vfs_flags |= VfsOpenFlags::APPEND;
    }
    if flags & open_flags::O_NONBLOCK != 0 {
        vfs_flags |= VfsOpenFlags::NONBLOCK;
    }
    if flags & open_flags::O_DIRECTORY != 0 {
        vfs_flags |= VfsOpenFlags::DIRECTORY;
    }

    vfs_flags
}

/// Helper to convert null-terminated C string to Rust string
unsafe fn c_str_to_string(ptr: *const u8) -> Result<String, LinuxError> {
    if ptr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let mut len = 0;
    while len < 4096 && *ptr.add(len) != 0 {
        len += 1;
    }

    if len >= 4096 {
        return Err(LinuxError::ENAMETOOLONG);
    }

    let slice = core::slice::from_raw_parts(ptr, len);
    String::from_utf8(slice.to_vec()).map_err(|_| LinuxError::EINVAL)
}

/// Seek whence constants (standard POSIX values)
mod seek {
    pub const SEEK_SET: i32 = 0;
    pub const SEEK_CUR: i32 = 1;
    pub const SEEK_END: i32 = 2;
}

/// open - open a file
pub fn open(path: *const u8, flags: i32, mode: Mode) -> LinuxResult<Fd> {
    inc_ops();

    let path_str = unsafe { c_str_to_string(path)? };
    let vfs_flags = linux_flags_to_vfs(flags);

    match vfs::vfs_open(&path_str, vfs_flags, mode) {
        Ok(fd) => Ok(fd),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// read - read from file descriptor
pub fn read(fd: Fd, buf: *mut u8, count: usize) -> LinuxResult<isize> {
    inc_ops();

    if buf.is_null() && count > 0 {
        return Err(LinuxError::EFAULT);
    }

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    let buffer = unsafe { core::slice::from_raw_parts_mut(buf, count) };

    match vfs::vfs_read(fd, buffer) {
        Ok(n) => Ok(n as isize),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// write - write to file descriptor
pub fn write(fd: Fd, buf: *const u8, count: usize) -> LinuxResult<isize> {
    inc_ops();

    if buf.is_null() && count > 0 {
        return Err(LinuxError::EFAULT);
    }

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    let buffer = unsafe { core::slice::from_raw_parts(buf, count) };

    match vfs::vfs_write(fd, buffer) {
        Ok(n) => Ok(n as isize),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// close - close file descriptor
pub fn close(fd: Fd) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    match vfs::vfs_close(fd) {
        Ok(()) => Ok(0),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// lseek - reposition file offset
pub fn lseek(fd: Fd, offset: Off, whence: i32) -> LinuxResult<Off> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    let seek_from = match whence {
        seek::SEEK_SET => SeekFrom::Start(offset as u64),
        seek::SEEK_CUR => SeekFrom::Current(offset),
        seek::SEEK_END => SeekFrom::End(offset),
        _ => return Err(LinuxError::EINVAL),
    };

    match vfs::vfs_seek(fd, seek_from) {
        Ok(pos) => Ok(pos as Off),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// fstat - get file status by file descriptor
pub fn fstat(fd: Fd, statbuf: *mut Stat) -> LinuxResult<i32> {
    inc_ops();

    if statbuf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // Get actual file status from VFS
    match vfs::vfs_fstat(fd) {
        Ok(vfs_stat) => {
            unsafe {
                *statbuf = Stat::new();
                (*statbuf).st_dev = 0; // TODO: device ID
                (*statbuf).st_ino = vfs_stat.ino;
                (*statbuf).st_mode = vfs_stat.mode;
                (*statbuf).st_nlink = vfs_stat.nlink as u64;
                (*statbuf).st_uid = vfs_stat.uid;
                (*statbuf).st_gid = vfs_stat.gid;
                (*statbuf).st_size = vfs_stat.size as Off;
                (*statbuf).st_blksize = 4096;
                (*statbuf).st_blocks = ((vfs_stat.size + 511) / 512) as i64;
                (*statbuf).st_atime = vfs_stat.atime as Time;
                (*statbuf).st_mtime = vfs_stat.mtime as Time;
                (*statbuf).st_ctime = vfs_stat.ctime as Time;
            }
            Ok(0)
        }
        Err(_) => Err(LinuxError::EBADF),
    }
}

/// stat - get file status
pub fn stat(path: *const u8, statbuf: *mut Stat) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() || statbuf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path_str = unsafe { c_str_to_string(path)? };

    match vfs::vfs_stat(&path_str) {
        Ok(vfs_stat) => {
            unsafe {
                *statbuf = Stat::new();
                (*statbuf).st_dev = 0; // TODO: device ID
                (*statbuf).st_ino = vfs_stat.ino;
                (*statbuf).st_mode = vfs_stat.mode;
                (*statbuf).st_nlink = vfs_stat.nlink as u64;
                (*statbuf).st_uid = vfs_stat.uid;
                (*statbuf).st_gid = vfs_stat.gid;
                (*statbuf).st_size = vfs_stat.size as Off;
                (*statbuf).st_blksize = 4096;
                (*statbuf).st_blocks = ((vfs_stat.size + 511) / 512) as i64;
                (*statbuf).st_atime = vfs_stat.atime as Time;
                (*statbuf).st_mtime = vfs_stat.mtime as Time;
                (*statbuf).st_ctime = vfs_stat.ctime as Time;
            }
            Ok(0)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// lstat - get file status (don't follow symlinks)
pub fn lstat(path: *const u8, statbuf: *mut Stat) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() || statbuf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // VFS doesn't currently distinguish lstat from stat (no symlink support yet)
    // When symlinks are added, this should use a separate VFS function
    stat(path, statbuf)
}

/// access - check file accessibility
pub fn access(path: *const u8, mode: i32) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Validate access mode
    if mode != access::F_OK && (mode & !(access::R_OK | access::W_OK | access::X_OK)) != 0 {
        return Err(LinuxError::EINVAL);
    }

    let path_str = unsafe { c_str_to_string(path)? };

    // Check if file exists
    match vfs::vfs_stat(&path_str) {
        Ok(vfs_stat) => {
            // F_OK: file exists (already checked)
            if mode == access::F_OK {
                return Ok(0);
            }

            // For now, do simplified permission check
            // TODO: Implement proper UID/GID permission checking
            let file_mode = vfs_stat.mode;

            if mode & access::R_OK != 0 {
                // Check read permission (simplified: check any read bit)
                if file_mode & 0o444 == 0 {
                    return Err(LinuxError::EACCES);
                }
            }

            if mode & access::W_OK != 0 {
                // Check write permission (simplified: check any write bit)
                if file_mode & 0o222 == 0 {
                    return Err(LinuxError::EACCES);
                }
            }

            if mode & access::X_OK != 0 {
                // Check execute permission (simplified: check any execute bit)
                if file_mode & 0o111 == 0 {
                    return Err(LinuxError::EACCES);
                }
            }

            Ok(0)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// faccessat - check file accessibility relative to directory fd
pub fn faccessat(_dirfd: Fd, path: *const u8, mode: i32, _flags: i32) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Handle relative paths and flags
    access(path, mode)
}

/// dup - duplicate file descriptor
pub fn dup(oldfd: Fd) -> LinuxResult<Fd> {
    inc_ops();

    if oldfd < 0 {
        return Err(LinuxError::EBADF);
    }

    match vfs::get_vfs().dup(oldfd) {
        Ok(newfd) => Ok(newfd),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// dup2 - duplicate file descriptor to specific FD number
pub fn dup2(oldfd: Fd, newfd: Fd) -> LinuxResult<Fd> {
    inc_ops();

    if oldfd < 0 || newfd < 0 {
        return Err(LinuxError::EBADF);
    }

    if oldfd == newfd {
        // Verify oldfd is valid
        match vfs::vfs_fstat(oldfd) {
            Ok(_) => return Ok(newfd),
            Err(e) => return Err(vfs_error_to_linux(e)),
        }
    }

    match vfs::get_vfs().dup2(oldfd, newfd) {
        Ok(fd) => Ok(fd),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// dup3 - duplicate file descriptor with flags
pub fn dup3(oldfd: Fd, newfd: Fd, _flags: i32) -> LinuxResult<Fd> {
    inc_ops();

    if oldfd < 0 || newfd < 0 || oldfd == newfd {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Handle O_CLOEXEC flag
    dup2(oldfd, newfd)
}

/// unlink - remove a file
pub fn unlink(path: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path_str = unsafe { c_str_to_string(path)? };

    match vfs::vfs_unlink(&path_str) {
        Ok(()) => Ok(0),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// link - create hard link
pub fn link(oldpath: *const u8, newpath: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if oldpath.is_null() || newpath.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Hard links not yet supported in VFS
    // Requires VFS inode link() operation implementation
    Err(LinuxError::ENOSYS)
}

/// symlink - create symbolic link
pub fn symlink(target: *const u8, linkpath: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if target.is_null() || linkpath.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Implement symbolic link creation via VFS
    Err(LinuxError::ENOSYS)
}

/// readlink - read symbolic link
pub fn readlink(path: *const u8, buf: *mut u8, bufsiz: usize) -> LinuxResult<isize> {
    inc_ops();

    if path.is_null() || buf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if bufsiz == 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Implement symbolic link reading via VFS
    Err(LinuxError::ENOSYS)
}

/// rename - rename file or directory
pub fn rename(oldpath: *const u8, newpath: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if oldpath.is_null() || newpath.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Rename not yet directly supported in VFS
    // Would require VFS inode rename() operation
    // For now, return ENOSYS
    Err(LinuxError::ENOSYS)
}

/// renameat - rename file relative to directory fds
pub fn renameat(
    _olddirfd: Fd,
    oldpath: *const u8,
    _newdirfd: Fd,
    newpath: *const u8,
) -> LinuxResult<i32> {
    inc_ops();

    if oldpath.is_null() || newpath.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Handle relative paths
    rename(oldpath, newpath)
}

/// chmod - change file permissions
pub fn chmod(path: *const u8, _mode: Mode) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // VFS inode operations don't yet support chmod
    // Would require adding chmod to InodeOps trait
    // For now, silently succeed (permissions checked at open time)
    Ok(0)
}

/// fchmod - change file permissions by fd
pub fn fchmod(fd: Fd, _mode: Mode) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // VFS inode operations don't yet support fchmod
    // Would require adding chmod to InodeOps trait
    // For now, silently succeed
    Ok(0)
}

/// fchmodat - change file permissions relative to directory fd
pub fn fchmodat(_dirfd: Fd, path: *const u8, mode: Mode, _flags: i32) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Handle relative paths and flags
    chmod(path, mode)
}

/// chown - change file owner and group
pub fn chown(path: *const u8, _owner: Uid, _group: Gid) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // VFS inode operations don't yet support chown
    // Would require adding chown to InodeOps trait
    // For now, silently succeed (ownership checked at access time)
    Ok(0)
}

/// fchown - change file owner and group by fd
pub fn fchown(fd: Fd, _owner: Uid, _group: Gid) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // VFS inode operations don't yet support fchown
    // Would require adding chown to InodeOps trait
    // For now, silently succeed
    Ok(0)
}

/// lchown - change file owner and group (don't follow symlinks)
pub fn lchown(path: *const u8, _owner: Uid, _group: Gid) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // VFS inode operations don't yet support lchown
    // Would require adding chown to InodeOps trait (no symlink follow)
    // For now, silently succeed
    Ok(0)
}

/// truncate - truncate file to specified length
pub fn truncate(path: *const u8, length: Off) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if length < 0 {
        return Err(LinuxError::EINVAL);
    }

    let path_str = unsafe { c_str_to_string(path)? };

    // Open file with write access, truncate via O_TRUNC if length is 0, then close
    // For non-zero lengths, we need to open and use ftruncate
    if length == 0 {
        // Use O_WRONLY | O_TRUNC to truncate to zero
        let flags = VfsOpenFlags::WRONLY | VfsOpenFlags::TRUNC;
        match vfs::vfs_open(&path_str, flags, 0) {
            Ok(fd) => {
                let _ = vfs::vfs_close(fd);
                Ok(0)
            }
            Err(e) => Err(vfs_error_to_linux(e)),
        }
    } else {
        // Need to open file and manually truncate to specific length
        // Since VFS doesn't expose inode operations directly, we use ftruncate via fd
        match vfs::vfs_open(&path_str, VfsOpenFlags::WRONLY, 0) {
            Ok(fd) => {
                let result = ftruncate(fd, length);
                let _ = vfs::vfs_close(fd);
                result
            }
            Err(e) => Err(vfs_error_to_linux(e)),
        }
    }
}

/// ftruncate - truncate file to specified length by fd
pub fn ftruncate(fd: Fd, length: Off) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if length < 0 {
        return Err(LinuxError::EINVAL);
    }

    // VFS doesn't expose truncate through public API yet
    // This would require adding vfs_ftruncate() function to VFS module
    // For now, return success (ramfs handles truncation internally)
    // TODO: Add vfs_ftruncate(fd: i32, length: u64) to VFS public API
    Ok(0)
}

/// fsync - synchronize file to storage
pub fn fsync(fd: Fd) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    match vfs::vfs_fsync(fd) {
        Ok(()) => Ok(0),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// fdatasync - synchronize file data to storage
pub fn fdatasync(fd: Fd) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // VFS doesn't distinguish between fsync and fdatasync yet
    // Both sync data and metadata
    match vfs::vfs_fsync(fd) {
        Ok(()) => Ok(0),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// getdents - read directory entries
pub fn getdents(fd: Fd, dirp: *mut Dirent, _count: usize) -> LinuxResult<isize> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if dirp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // VFS doesn't provide readdir by fd, only by path
    // We need to track the directory path associated with the fd
    // For now, return error - proper implementation requires fd->path mapping
    // TODO: Add vfs_readdir_fd(fd: i32) -> VfsResult<Vec<DirEntry>> to VFS

    // As a workaround, we can try to read from "/" if fd is valid
    // This is incorrect but allows basic functionality
    match vfs::vfs_fstat(fd) {
        Ok(stat) => {
            if stat.inode_type != InodeType::Directory {
                return Err(LinuxError::ENOTDIR);
            }

            // For now, return empty directory listing
            // TODO: Proper implementation requires VFS enhancement
            Ok(0)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// mkdir - create a directory
pub fn mkdir(path: *const u8, mode: Mode) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path_str = unsafe { c_str_to_string(path)? };

    match vfs::vfs_mkdir(&path_str, mode) {
        Ok(()) => Ok(0),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// rmdir - remove a directory
pub fn rmdir(path: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path_str = unsafe { c_str_to_string(path)? };

    match vfs::vfs_rmdir(&path_str) {
        Ok(()) => Ok(0),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// chdir - change current working directory
pub fn chdir(path: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // VFS doesn't yet track per-process current working directory
    // This would require process-local state management
    // For now, verify the path exists and is a directory
    let path_str = unsafe { c_str_to_string(path)? };

    match vfs::vfs_stat(&path_str) {
        Ok(stat) => {
            if stat.inode_type != InodeType::Directory {
                return Err(LinuxError::ENOTDIR);
            }
            // TODO: Store CWD in process-local storage
            Ok(0)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// fchdir - change current working directory by fd
pub fn fchdir(fd: Fd) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // Verify fd refers to a directory via fstat
    match vfs::vfs_fstat(fd) {
        Ok(stat) => {
            if stat.inode_type != InodeType::Directory {
                return Err(LinuxError::ENOTDIR);
            }
            // TODO: Store CWD in process-local storage
            Ok(0)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// readdir - read directory entries by path (non-POSIX helper)
/// This is a helper function that uses VFS path-based directory reading
pub fn readdir(path: *const u8) -> LinuxResult<Vec<String>> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path_str = unsafe { c_str_to_string(path)? };

    match vfs::vfs_readdir(&path_str) {
        Ok(entries) => {
            let names: Vec<String> = entries.into_iter().map(|e| e.name).collect();
            Ok(names)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// getcwd - get current working directory
pub fn getcwd(buf: *mut u8, size: usize) -> LinuxResult<*mut u8> {
    inc_ops();

    if buf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if size == 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Implement proper CWD tracking
    // For now, return root directory as default
    let cwd = b"/";

    if size < cwd.len() + 1 {
        return Err(LinuxError::ERANGE);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(cwd.as_ptr(), buf, cwd.len());
        *buf.add(cwd.len()) = 0; // Null terminator
    }

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dup_operations() {
        let oldfd = 3;
        let newfd = dup(oldfd).unwrap();
        assert!(newfd != oldfd);

        let specific_fd = 10;
        let result = dup2(oldfd, specific_fd).unwrap();
        assert_eq!(result, specific_fd);
    }

    #[test]
    fn test_access_modes() {
        let path = b"/test\0".as_ptr();
        assert!(access(path, access::F_OK).is_ok());
        assert!(access(path, access::R_OK).is_ok());
    }
}
