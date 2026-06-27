//! Linux file operation APIs
//!
//! This module implements Linux-compatible file operations including
//! stat, access, dup, link operations, and directory handling.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::process;
use crate::vfs::{self, InodeType, OpenFlags as VfsOpenFlags, SeekFrom, VfsError};

/// AT_FDCWD — use process cwd for relative paths
pub const AT_FDCWD: Fd = -100;

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

/// openat - open a file relative to a directory fd
pub fn openat(dirfd: Fd, pathname: *const u8, flags: i32, mode: Mode) -> LinuxResult<Fd> {
    inc_ops();

    if pathname.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let resolved_path = resolve_at_path(dirfd, pathname)?;
    let vfs_flags = linux_flags_to_vfs(flags);

    match vfs::vfs_open(&resolved_path, vfs_flags, mode) {
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

    if let Some(result) = super::special_fd::try_read(fd, buffer) {
        return result;
    }

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

    if let Some(result) = super::special_fd::try_write(fd, buffer) {
        return result;
    }

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
pub fn faccessat(dirfd: Fd, path: *const u8, mode: i32, flags: i32) -> LinuxResult<i32> {
    faccessat2(dirfd, path, mode, flags)
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

    let old = unsafe { c_str_to_string(oldpath)? };
    let new = unsafe { c_str_to_string(newpath)? };

    match vfs::vfs_link(&old, &new) {
        Ok(_) => Ok(0),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// symlink - create symbolic link
pub fn symlink(target: *const u8, linkpath: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if target.is_null() || linkpath.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let target = unsafe { c_str_to_string(target)? };
    let linkpath = unsafe { c_str_to_string(linkpath)? };

    match vfs::vfs_symlink(&target, &linkpath) {
        Ok(_) => Ok(0),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
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

    let path = unsafe { c_str_to_string(path)? };

    match vfs::vfs_readlink(&path) {
        Ok(target) => {
            let bytes = target.as_bytes();
            let n = core::cmp::min(bytes.len(), bufsiz);
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, n);
            }
            Ok(n as isize)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// rename - rename file or directory
pub fn rename(oldpath: *const u8, newpath: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if oldpath.is_null() || newpath.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let old = unsafe { c_str_to_string(oldpath)? };
    let new = unsafe { c_str_to_string(newpath)? };

    match vfs::vfs_rename(&old, &new) {
        Ok(_) => Ok(0),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// renameat - rename file relative to directory fds
pub fn renameat(
    olddirfd: Fd,
    oldpath: *const u8,
    newdirfd: Fd,
    newpath: *const u8,
) -> LinuxResult<i32> {
    renameat2(olddirfd, oldpath, newdirfd, newpath, 0)
}

/// chmod - change file permissions
pub fn chmod(path: *const u8, mode: Mode) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path = unsafe { c_str_to_string(path)? };
    vfs::vfs_chmod(&path, mode).map_err(vfs_error_to_linux)?;
    Ok(0)
}

/// fchmod - change file permissions by fd
pub fn fchmod(fd: Fd, mode: Mode) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    vfs::vfs_fchmod(fd, mode).map_err(vfs_error_to_linux)?;
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
pub fn chown(path: *const u8, owner: Uid, group: Gid) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path = unsafe { c_str_to_string(path)? };
    vfs::vfs_chown(&path, owner, group).map_err(vfs_error_to_linux)?;
    Ok(0)
}

/// fchown - change file owner and group by fd
pub fn fchown(fd: Fd, owner: Uid, group: Gid) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    vfs::vfs_fchown(fd, owner, group).map_err(vfs_error_to_linux)?;
    Ok(0)
}

/// lchown - change file owner and group (don't follow symlinks)
pub fn lchown(path: *const u8, owner: Uid, group: Gid) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path = unsafe { c_str_to_string(path)? };
    vfs::vfs_chown(&path, owner, group).map_err(vfs_error_to_linux)?;
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
    match vfs::vfs_ftruncate(fd, length as u64) {
        Ok(()) => Ok(0),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
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
pub fn getdents(fd: Fd, dirp: *mut Dirent, count: usize) -> LinuxResult<isize> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if dirp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let (entries, cookie) = vfs::vfs_readdir_fd(fd).map_err(vfs_error_to_linux)?;

    let mut written = 0usize;
    let mut index = cookie as usize;

    while index < entries.len() {
        let entry = &entries[index];
        let name_bytes = entry.name.as_bytes();
        let reclen = core::mem::size_of::<Dirent>() as u16;
        if written + reclen as usize > count {
            break;
        }

        let d_type = match entry.inode_type {
            InodeType::Directory => 4,
            InodeType::File => 8,
            InodeType::Symlink => 10,
            InodeType::CharDevice => 2,
            InodeType::BlockDevice => 6,
            InodeType::Fifo => 1,
            InodeType::Socket => 12,
        };

        unsafe {
            let dent = &mut *(dirp.add(written) as *mut Dirent);
            dent.d_ino = entry.ino as Ino;
            dent.d_off = (index + 1) as Off;
            dent.d_reclen = reclen;
            dent.d_type = d_type;
            let name_len = core::cmp::min(name_bytes.len(), 255);
            dent.d_name[..name_len].copy_from_slice(&name_bytes[..name_len]);
            if name_len < 256 {
                dent.d_name[name_len] = 0;
            }
        }

        written += reclen as usize;
        index += 1;
    }

    let _ = vfs::vfs_set_dir_cookie(fd, index as u64);
    Ok(written as isize)
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
            let pid = process::current_pid();
            if process::get_process_manager()
                .with_process_mut(pid, |pcb| {
                    pcb.cwd = path_str;
                })
                .is_some()
            {
                Ok(0)
            } else {
                Err(LinuxError::ESRCH)
            }
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

    let pid = process::current_pid();
    let cwd = process::get_process_manager()
        .get_process(pid)
        .map(|pcb| pcb.cwd)
        .unwrap_or_else(|| String::from("/"));
    let cwd_bytes = cwd.as_bytes();

    if size < cwd_bytes.len() + 1 {
        return Err(LinuxError::ERANGE);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(cwd_bytes.as_ptr(), buf, cwd_bytes.len());
        *buf.add(cwd_bytes.len()) = 0;
    }

    Ok(buf)
}

/// Helper to resolve relative path from directory fd
fn resolve_at_path(dirfd: Fd, pathname: *const u8) -> LinuxResult<String> {
    if pathname.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path_str = unsafe { c_str_to_string(pathname)? };
    if path_str.starts_with('/') || dirfd == AT_FDCWD {
        return Ok(path_str);
    }

    // Check that dirfd is a directory
    match vfs::vfs_fstat(dirfd) {
        Ok(stat) => {
            if stat.inode_type != InodeType::Directory {
                return Err(LinuxError::ENOTDIR);
            }
        }
        Err(e) => return Err(vfs_error_to_linux(e)),
    }

    let pid = process::current_pid();
    let cwd = process::get_process_manager()
        .get_process(pid)
        .map(|pcb| pcb.cwd.clone())
        .ok_or(LinuxError::ESRCH)?;

    let full_path = if cwd.ends_with('/') {
        alloc::format!("{}{}", cwd, path_str)
    } else {
        alloc::format!("{}/{}", cwd, path_str)
    };

    Ok(full_path)
}

/// openat2 - open a file with extended flags and attributes
pub fn openat2(
    dirfd: Fd,
    pathname: *const u8,
    how: *const OpenHow,
    size: usize,
) -> LinuxResult<Fd> {
    inc_ops();

    if pathname.is_null() || how.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let expected_size = core::mem::size_of::<OpenHow>();
    if size < expected_size {
        return Err(LinuxError::EINVAL);
    }

    let open_how = unsafe { &*how };
    openat(
        dirfd,
        pathname,
        open_how.flags as i32,
        open_how.mode as Mode,
    )
}

fn populate_statx(vfs_stat: &vfs::Stat, statxbuf: *mut Statx) {
    unsafe {
        core::ptr::write_bytes(statxbuf, 0, 1);
        let s = &mut *statxbuf;
        s.stx_mask = 0x7ff; // STATX_BASIC_STATS
        s.stx_blksize = 4096;
        s.stx_nlink = vfs_stat.nlink;
        s.stx_uid = vfs_stat.uid;
        s.stx_gid = vfs_stat.gid;
        s.stx_mode = vfs_stat.mode as u16;
        s.stx_ino = vfs_stat.ino;
        s.stx_size = vfs_stat.size;
        s.stx_blocks = ((vfs_stat.size + 511) / 512) as u64;

        s.stx_atime = StatxTimestamp {
            tv_sec: vfs_stat.atime as i64,
            tv_nsec: 0,
            __reserved: 0,
        };
        s.stx_mtime = StatxTimestamp {
            tv_sec: vfs_stat.mtime as i64,
            tv_nsec: 0,
            __reserved: 0,
        };
        s.stx_ctime = StatxTimestamp {
            tv_sec: vfs_stat.ctime as i64,
            tv_nsec: 0,
            __reserved: 0,
        };
        s.stx_btime = StatxTimestamp {
            tv_sec: vfs_stat.ctime as i64,
            tv_nsec: 0,
            __reserved: 0,
        };
    }
}

/// statx - get detailed file status relative to directory fd
pub fn statx(
    dirfd: Fd,
    pathname: *const u8,
    flags: i32,
    _mask: u32,
    statxbuf: *mut Statx,
) -> LinuxResult<i32> {
    inc_ops();

    if statxbuf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let is_empty_path = (flags & 0x1000) != 0; // AT_EMPTY_PATH

    if pathname.is_null() || (is_empty_path && unsafe { *pathname == 0 }) {
        if dirfd == AT_FDCWD {
            let pid = process::current_pid();
            let cwd = process::get_process_manager()
                .get_process(pid)
                .map(|pcb| pcb.cwd.clone())
                .ok_or(LinuxError::ESRCH)?;
            match vfs::vfs_stat(&cwd) {
                Ok(vfs_stat) => {
                    populate_statx(&vfs_stat, statxbuf);
                    return Ok(0);
                }
                Err(e) => return Err(vfs_error_to_linux(e)),
            }
        } else {
            match vfs::vfs_fstat(dirfd) {
                Ok(vfs_stat) => {
                    populate_statx(&vfs_stat, statxbuf);
                    return Ok(0);
                }
                Err(e) => return Err(vfs_error_to_linux(e)),
            }
        }
    }

    let resolved_path = resolve_at_path(dirfd, pathname)?;

    match vfs::vfs_stat(&resolved_path) {
        Ok(vfs_stat) => {
            populate_statx(&vfs_stat, statxbuf);
            Ok(0)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// faccessat2 - check file accessibility relative to directory fd with flags
pub fn faccessat2(dirfd: Fd, path: *const u8, mode: i32, _flags: i32) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if mode != access::F_OK && (mode & !(access::R_OK | access::W_OK | access::X_OK)) != 0 {
        return Err(LinuxError::EINVAL);
    }

    let resolved_path = resolve_at_path(dirfd, path)?;

    match vfs::vfs_stat(&resolved_path) {
        Ok(vfs_stat) => {
            if mode == access::F_OK {
                return Ok(0);
            }

            let file_mode = vfs_stat.mode;

            if mode & access::R_OK != 0 {
                if file_mode & 0o444 == 0 {
                    return Err(LinuxError::EACCES);
                }
            }

            if mode & access::W_OK != 0 {
                if file_mode & 0o222 == 0 {
                    return Err(LinuxError::EACCES);
                }
            }

            if mode & access::X_OK != 0 {
                if file_mode & 0o111 == 0 {
                    return Err(LinuxError::EACCES);
                }
            }

            Ok(0)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// renameat2 - rename file relative to directory fds with flags
pub fn renameat2(
    olddirfd: Fd,
    oldpath: *const u8,
    newdirfd: Fd,
    newpath: *const u8,
    flags: u32,
) -> LinuxResult<i32> {
    inc_ops();

    if oldpath.is_null() || newpath.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let resolved_old = resolve_at_path(olddirfd, oldpath)?;
    let resolved_new = resolve_at_path(newdirfd, newpath)?;

    // We only support basic rename for now (flags are not supported by VFS yet)
    let _ = flags;

    match vfs::vfs_rename(&resolved_old, &resolved_new) {
        Ok(_) => Ok(0),
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// readlinkat - read value of symbolic link relative to directory fd
pub fn readlinkat(dirfd: Fd, path: *const u8, buf: *mut u8, bufsiz: usize) -> LinuxResult<isize> {
    inc_ops();

    if path.is_null() || buf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if bufsiz == 0 {
        return Err(LinuxError::EINVAL);
    }

    let resolved_path = resolve_at_path(dirfd, path)?;
    let temp_c_str = alloc::format!("{}\0", resolved_path);
    readlink(temp_c_str.as_ptr(), buf, bufsiz)
}

/// mkdirat - create directory relative to directory fd
pub fn mkdirat(dirfd: Fd, path: *const u8, mode: Mode) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let resolved_path = resolve_at_path(dirfd, path)?;
    let temp_c_str = alloc::format!("{}\0", resolved_path);
    mkdir(temp_c_str.as_ptr(), mode)
}

/// unlinkat - remove directory entry relative to directory fd
pub fn unlinkat(dirfd: Fd, path: *const u8, flags: i32) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let resolved_path = resolve_at_path(dirfd, path)?;
    let temp_c_str = alloc::format!("{}\0", resolved_path);
    if flags & 0x200 != 0 {
        // AT_REMOVEDIR - use rmdir instead
        rmdir(temp_c_str.as_ptr())
    } else {
        unlink(temp_c_str.as_ptr())
    }
}

/// linkat - create hard link relative to directory fds
pub fn linkat(
    olddirfd: Fd,
    oldpath: *const u8,
    newdirfd: Fd,
    newpath: *const u8,
    flags: i32,
) -> LinuxResult<i32> {
    inc_ops();

    if oldpath.is_null() || newpath.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let resolved_old = resolve_at_path(olddirfd, oldpath)?;
    let resolved_new = resolve_at_path(newdirfd, newpath)?;
    let old_c_str = alloc::format!("{}\0", resolved_old);
    let new_c_str = alloc::format!("{}\0", resolved_new);

    let _ = flags;
    link(old_c_str.as_ptr(), new_c_str.as_ptr())
}

/// symlinkat - create symbolic link relative to directory fd
pub fn symlinkat(target: *const u8, newdirfd: Fd, linkpath: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if target.is_null() || linkpath.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let resolved_linkpath = resolve_at_path(newdirfd, linkpath)?;
    let linkpath_c_str = alloc::format!("{}\0", resolved_linkpath);
    symlink(target, linkpath_c_str.as_ptr())
}

/// fchownat - change file owner and group relative to directory fd
pub fn fchownat(
    dirfd: Fd,
    path: *const u8,
    owner: Uid,
    group: Gid,
    flags: i32,
) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let resolved_path = resolve_at_path(dirfd, path)?;
    let temp_c_str = alloc::format!("{}\0", resolved_path);

    let _ = flags;
    chown(temp_c_str.as_ptr(), owner, group)
}

/// utimensat - set file times relative to directory fd
pub fn utimensat(
    dirfd: Fd,
    path: *const u8,
    times: *const [crate::linux_compat::TimeSpec; 2],
    flags: i32,
) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let resolved_path = resolve_at_path(dirfd, path)?;
    match vfs::vfs_stat(&resolved_path) {
        Ok(_) => {
            let _ = (times, flags);
            Ok(0)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
}

/// fallocate - preallocate or deallocate space for a file
pub fn fallocate(fd: Fd, mode: i32, offset: Off, len: Off) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if offset < 0 || len <= 0 {
        return Err(LinuxError::EINVAL);
    }

    let _ = mode;
    Ok(0)
}

#[cfg(any())]
mod tests {
    use super::*;

    #[cfg(feature = "disabled-tests")]
    #[test_case]
    fn test_dup_operations() {
        let oldfd = 3;
        let newfd = dup(oldfd).unwrap();
        assert!(newfd != oldfd);

        let specific_fd = 10;
        let result = dup2(oldfd, specific_fd).unwrap();
        assert_eq!(result, specific_fd);
    }

    #[cfg(feature = "disabled-tests")]
    #[test_case]
    fn test_access_modes() {
        let path = b"/test\0".as_ptr();
        assert!(access(path, access::F_OK).is_ok());
        assert!(access(path, access::R_OK).is_ok());
    }
}
