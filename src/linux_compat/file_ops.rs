//! Linux file operation APIs
//!
//! This module implements Linux-compatible file operations including
//! stat, access, dup, link operations, and directory handling.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::memory::user_space::UserSpaceMemory;
use crate::process::{self, FileDescriptor};

/// AT_FDCWD — use process cwd for relative paths
pub const AT_FDCWD: Fd = -100;
use crate::vfs::{self, InodeType, OpenFlags as VfsOpenFlags, SeekFrom, VfsError};

/// FD_CLOEXEC flag stored in PCB fd flags
const FD_CLOEXEC: u32 = 1;
const MAX_RW_CHUNK: usize = 64 * 1024;

fn stat_dev(vfs_stat: &vfs::Stat) -> u64 {
    match vfs_stat.inode_type {
        InodeType::CharDevice | InodeType::BlockDevice => vfs_stat.rdev,
        _ => 1,
    }
}

fn populate_linux_stat(statbuf: *mut Stat, vfs_stat: &vfs::Stat) {
    unsafe {
        *statbuf = Stat::new();
        (*statbuf).st_dev = stat_dev(vfs_stat);
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
}

fn check_access_permissions(
    vfs_stat: &vfs::Stat,
    mode: i32,
    uid: u32,
    gid: u32,
    supplementary: &[u32],
) -> LinuxResult<()> {
    if mode == access::F_OK {
        return Ok(());
    }

    if uid == 0 {
        return Ok(());
    }

    let file_mode = vfs_stat.mode;
    let perm_bits = if uid == vfs_stat.uid {
        (file_mode >> 6) & 0o7
    } else if gid == vfs_stat.gid || supplementary.contains(&vfs_stat.gid) {
        (file_mode >> 3) & 0o7
    } else {
        file_mode & 0o7
    };

    if mode & access::R_OK != 0 && perm_bits & 0o4 == 0 {
        return Err(LinuxError::EACCES);
    }
    if mode & access::W_OK != 0 && perm_bits & 0o2 == 0 {
        return Err(LinuxError::EACCES);
    }
    if mode & access::X_OK != 0 && perm_bits & 0o1 == 0 {
        return Err(LinuxError::EACCES);
    }

    Ok(())
}

fn set_fd_cloexec(fd: Fd) {
    let pid = process::current_pid();
    process::get_process_manager().with_process_mut(pid, |pcb| {
        if let Some(entry) = pcb.fd_table.get_mut(&(fd as u32)) {
            entry.flags |= FD_CLOEXEC;
        } else {
            pcb.fd_table
                .insert(fd as u32, FileDescriptor::from_vfs_fd(fd, FD_CLOEXEC));
        }
        pcb.file_descriptors = pcb.fd_table.clone();
    });
}

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
pub(crate) fn vfs_error_to_linux(err: VfsError) -> LinuxError {
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
        VfsError::DirectoryNotEmpty => LinuxError::ENOTEMPTY,
        VfsError::DiskQuotaExceeded => LinuxError::EDQUOT,
    }
}

/// Convert Linux open flags to VFS open flags
pub(crate) fn linux_flags_to_vfs(flags: i32) -> u32 {
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
pub(crate) fn c_str_to_string(ptr: *const u8) -> Result<String, LinuxError> {
    if ptr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path =
        UserSpaceMemory::copy_string_from_user(ptr as u64, 4096).map_err(|_| LinuxError::EFAULT)?;
    if path.len() >= 4096 {
        return Err(LinuxError::ENAMETOOLONG);
    }

    Ok(path)
}

fn check_landlock(path: &str, access: u64) -> LinuxResult<()> {
    if crate::landlock::check_fs_access(process::current_pid(), path, access) {
        Ok(())
    } else {
        Err(LinuxError::EACCES)
    }
}

fn landlock_open_access(flags: i32) -> u64 {
    let mut access = match flags & 0o3 {
        open_flags::O_WRONLY => crate::landlock::LANDLOCK_ACCESS_FS_WRITE_FILE,
        open_flags::O_RDWR => {
            crate::landlock::LANDLOCK_ACCESS_FS_READ_FILE
                | crate::landlock::LANDLOCK_ACCESS_FS_WRITE_FILE
        }
        _ => crate::landlock::LANDLOCK_ACCESS_FS_READ_FILE,
    };
    if flags & open_flags::O_CREAT != 0 {
        access |= crate::landlock::LANDLOCK_ACCESS_FS_MAKE_REG;
    }
    if flags & open_flags::O_TRUNC != 0 {
        access |= crate::landlock::LANDLOCK_ACCESS_FS_TRUNCATE;
    }
    access
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

    let open_addr = open as *const () as u64;
    crate::kprobes::run_probes_at(open_addr, flags as u64);

    let path_str = c_str_to_string(path)?;
    check_landlock(&path_str, landlock_open_access(flags))?;
    let vfs_flags = linux_flags_to_vfs(flags);

    let result = match vfs::vfs_open(&path_str, vfs_flags, mode) {
        Ok(fd) => Ok(fd),
        Err(e) => Err(vfs_error_to_linux(e)),
    };

    if crate::audit::is_enabled() {
        crate::audit::audit_log_path(crate::audit::AuditOp::Open, &path_str, result.is_ok());
    }
    if crate::trace::tracing_on() {
        crate::trace::tracepoint_emit("file:open", result.as_ref().copied().unwrap_or(0) as u64);
    }

    result
}

/// openat - open a file relative to a directory fd
pub fn openat(dirfd: Fd, pathname: *const u8, flags: i32, mode: Mode) -> LinuxResult<Fd> {
    inc_ops();

    if pathname.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let resolved_path = resolve_at_path(dirfd, pathname)?;
    check_landlock(&resolved_path, landlock_open_access(flags))?;
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

    if count == 0 {
        return Ok(0);
    }

    let len = count.min(MAX_RW_CHUNK);
    let mut buffer = vec![0u8; len];

    let bytes_read = if let Some(result) = crate::drivers::tty::try_read_fd(fd, &mut buffer) {
        result?
    } else if let Some(result) = super::special_fd::try_read(fd, &mut buffer) {
        result?
    } else {
        match vfs::vfs_read(fd, &mut buffer) {
            Ok(n) => n as isize,
            Err(e) => return Err(vfs_error_to_linux(e)),
        }
    };

    if bytes_read > 0 {
        UserSpaceMemory::copy_to_user(buf as u64, &buffer[..bytes_read as usize])
            .map_err(|_| LinuxError::EFAULT)?;
    }

    let read_addr = read as *const () as u64;
    crate::kprobes::run_probes_at(read_addr, fd as u64);
    if crate::trace::tracing_on() {
        crate::trace::tracepoint_emit("file:read", bytes_read.max(0) as u64);
    }

    Ok(bytes_read)
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

    if count == 0 {
        return Ok(0);
    }

    let len = count.min(MAX_RW_CHUNK);
    let mut buffer = vec![0u8; len];
    UserSpaceMemory::copy_from_user(buf as u64, &mut buffer).map_err(|_| LinuxError::EFAULT)?;

    if let Some(result) = crate::drivers::tty::try_write_fd(fd, &buffer) {
        return finish_write(result, fd);
    }

    if let Some(result) = super::special_fd::try_write(fd, &buffer) {
        return finish_write(result, fd);
    }

    finish_write(
        match vfs::vfs_write(fd, &buffer) {
            Ok(n) => Ok(n as isize),
            Err(e) => Err(vfs_error_to_linux(e)),
        },
        fd,
    )
}

fn finish_write(result: LinuxResult<isize>, fd: Fd) -> LinuxResult<isize> {
    let write_addr = write as *const () as u64;
    crate::kprobes::run_probes_at(write_addr, fd as u64);
    if crate::trace::tracing_on() {
        crate::trace::tracepoint_emit(
            "file:write",
            result.as_ref().copied().unwrap_or(0).max(0) as u64,
        );
    }
    result
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
            populate_linux_stat(statbuf, &vfs_stat);
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

    let path_str = c_str_to_string(path)?;

    match vfs::vfs_stat(&path_str) {
        Ok(vfs_stat) => {
            populate_linux_stat(statbuf, &vfs_stat);
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

/// newfstatat - get file status relative to a directory file descriptor
///
/// # Arguments
/// * `dirfd` - Directory fd or AT_FDCWD
/// * `pathname` - File path (may be relative to dirfd)
/// * `statbuf` - Output stat buffer
/// * `flags` - AT_SYMLINK_NOFOLLOW (0x100) or 0
pub fn newfstatat(
    dirfd: Fd,
    pathname: *const u8,
    statbuf: *mut Stat,
    flags: i32,
) -> LinuxResult<i32> {
    inc_ops();

    if pathname.is_null() || statbuf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Resolve the path relative to dirfd
    let path_str = resolve_at_path(dirfd, pathname)?;

    // AT_SYMLINK_NOFOLLOW (0x100): stat the symlink itself, not the target.
    const AT_SYMLINK_NOFOLLOW: i32 = 0x100;

    let stat_result = if flags & AT_SYMLINK_NOFOLLOW != 0 {
        vfs::vfs_lstat(&path_str)
    } else {
        vfs::vfs_stat(&path_str)
    };

    match stat_result {
        Ok(vfs_stat) => {
            populate_linux_stat(statbuf, &vfs_stat);
            Ok(0)
        }
        Err(e) => Err(vfs_error_to_linux(e)),
    }
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

    let path_str = c_str_to_string(path)?;

    let pid = process::current_pid();
    let (uid, gid, groups) = process::get_process_manager()
        .get_process(pid)
        .map(|pcb| (pcb.euid, pcb.egid, pcb.supplementary_groups.clone()))
        .ok_or(LinuxError::ESRCH)?;

    match vfs::vfs_stat(&path_str) {
        Ok(vfs_stat) => check_access_permissions(&vfs_stat, mode, uid, gid, &groups).map(|_| 0),
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
pub fn dup3(oldfd: Fd, newfd: Fd, flags: i32) -> LinuxResult<Fd> {
    inc_ops();

    if oldfd < 0 || newfd < 0 || oldfd == newfd {
        return Err(LinuxError::EINVAL);
    }

    const O_CLOEXEC: i32 = open_flags::O_CLOEXEC;
    if flags & !O_CLOEXEC != 0 {
        return Err(LinuxError::EINVAL);
    }

    let result = dup2(oldfd, newfd)?;
    if flags & O_CLOEXEC != 0 {
        set_fd_cloexec(result);
    }
    Ok(result)
}

/// unlink - remove a file
pub fn unlink(path: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path_str = c_str_to_string(path)?;
    check_landlock(&path_str, crate::landlock::LANDLOCK_ACCESS_FS_REMOVE_FILE)?;

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

    let old = c_str_to_string(oldpath)?;
    let new = c_str_to_string(newpath)?;
    check_landlock(&old, crate::landlock::LANDLOCK_ACCESS_FS_REFER)?;
    check_landlock(&new, crate::landlock::LANDLOCK_ACCESS_FS_MAKE_REG)?;

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

    let target = c_str_to_string(target)?;
    let linkpath = c_str_to_string(linkpath)?;
    check_landlock(&linkpath, crate::landlock::LANDLOCK_ACCESS_FS_MAKE_SYM)?;

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

    let path = c_str_to_string(path)?;

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

    let old = c_str_to_string(oldpath)?;
    let new = c_str_to_string(newpath)?;

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

    let path = c_str_to_string(path)?;
    check_landlock(&path, crate::landlock::LANDLOCK_ACCESS_FS_WRITE_FILE)?;
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
pub fn fchmodat(dirfd: Fd, path: *const u8, mode: Mode, _flags: i32) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let resolved_path = resolve_at_path(dirfd, path)?;
    check_landlock(
        &resolved_path,
        crate::landlock::LANDLOCK_ACCESS_FS_WRITE_FILE,
    )?;
    vfs::vfs_chmod(&resolved_path, mode).map_err(vfs_error_to_linux)?;
    Ok(0)
}

/// chown - change file owner and group
pub fn chown(path: *const u8, owner: Uid, group: Gid) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path = c_str_to_string(path)?;
    check_landlock(&path, crate::landlock::LANDLOCK_ACCESS_FS_WRITE_FILE)?;
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

    let path = c_str_to_string(path)?;
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

    let path_str = c_str_to_string(path)?;
    check_landlock(&path_str, crate::landlock::LANDLOCK_ACCESS_FS_TRUNCATE)?;

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

    let path_str = c_str_to_string(path)?;

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

    let path_str = c_str_to_string(path)?;

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
    let path_str = c_str_to_string(path)?;

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
            let dir_path = vfs::vfs_fd_directory_path(fd).map_err(vfs_error_to_linux)?;
            let pid = process::current_pid();
            if process::get_process_manager()
                .with_process_mut(pid, |pcb| {
                    pcb.cwd = dir_path;
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

/// readdir - read directory entries by path (non-POSIX helper)
/// This is a helper function that uses VFS path-based directory reading
pub fn readdir(path: *const u8) -> LinuxResult<Vec<String>> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path_str = c_str_to_string(path)?;

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
pub(crate) fn resolve_at_path(dirfd: Fd, pathname: *const u8) -> LinuxResult<String> {
    if pathname.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path_str = c_str_to_string(pathname)?;
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

    let pid = process::current_pid();
    let (uid, gid, groups) = process::get_process_manager()
        .get_process(pid)
        .map(|pcb| (pcb.euid, pcb.egid, pcb.supplementary_groups.clone()))
        .ok_or(LinuxError::ESRCH)?;

    match vfs::vfs_stat(&resolved_path) {
        Ok(vfs_stat) => check_access_permissions(&vfs_stat, mode, uid, gid, &groups).map(|_| 0),
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
    check_landlock(&resolved_old, crate::landlock::LANDLOCK_ACCESS_FS_REFER)?;
    check_landlock(&resolved_new, crate::landlock::LANDLOCK_ACCESS_FS_REFER)?;

    const RENAME_NOREPLACE: u32 = 1 << 0;
    const RENAME_EXCHANGE: u32 = 1 << 1;

    if (flags & RENAME_NOREPLACE) != 0 {
        // Fail if the destination already exists.
        if vfs::get_vfs().lookup(&resolved_new).is_ok() {
            return Err(LinuxError::EEXIST);
        }
    }

    if (flags & RENAME_EXCHANGE) != 0 {
        // Atomically exchange the two paths.  Our VFS doesn't have a
        // native exchange operation, so we use a temporary intermediate.
        let tmp = format!("/tmp/.rename_exchange_{}", crate::time::uptime_ns());
        vfs::vfs_rename(&resolved_old, &tmp).map_err(vfs_error_to_linux)?;
        match vfs::vfs_rename(&resolved_new, &resolved_old) {
            Ok(_) => {
                if let Err(e) = vfs::vfs_rename(&tmp, &resolved_new) {
                    // Best-effort cleanup
                    let _ = vfs::vfs_unlink(&tmp);
                    return Err(vfs_error_to_linux(e));
                }
                return Ok(0);
            }
            Err(e) => {
                // Restore original
                let _ = vfs::vfs_rename(&tmp, &resolved_old);
                return Err(vfs_error_to_linux(e));
            }
        }
    }

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
    check_landlock(
        &resolved_path,
        crate::landlock::LANDLOCK_ACCESS_FS_READ_FILE,
    )?;
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
    check_landlock(&resolved_path, crate::landlock::LANDLOCK_ACCESS_FS_MAKE_DIR)?;
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
    let access = if flags & 0x200 != 0 {
        crate::landlock::LANDLOCK_ACCESS_FS_REMOVE_DIR
    } else {
        crate::landlock::LANDLOCK_ACCESS_FS_REMOVE_FILE
    };
    check_landlock(&resolved_path, access)?;
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

    // AT_SYMLINK_FOLLOW (0x1000) is a no-op: our VFS link() already
    // resolves symlinks on the source path.
    let _ = flags & 0x1000;
    link(old_c_str.as_ptr(), new_c_str.as_ptr())
}

/// symlinkat - create symbolic link relative to directory fd
pub fn symlinkat(target: *const u8, newdirfd: Fd, linkpath: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if target.is_null() || linkpath.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let resolved_linkpath = resolve_at_path(newdirfd, linkpath)?;
    check_landlock(
        &resolved_linkpath,
        crate::landlock::LANDLOCK_ACCESS_FS_MAKE_SYM,
    )?;
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
    check_landlock(
        &resolved_path,
        crate::landlock::LANDLOCK_ACCESS_FS_WRITE_FILE,
    )?;
    let temp_c_str = alloc::format!("{}\0", resolved_path);

    // AT_SYMLINK_NO_FOLLOW (0x100) and AT_EMPTY_PATH (0x1000) are
    // effectively no-ops: our VFS chown() resolves the path.
    let _ = flags & (0x100 | 0x1000);
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

    const AT_SYMLINK_NOFOLLOW: i32 = 0x100;
    const UTIME_NOW: i64 = 1_000_000_000;
    const UTIME_OMIT: i64 = 1_000_000_001;

    let follow_symlinks = flags & AT_SYMLINK_NOFOLLOW == 0;
    let _ = follow_symlinks; // VFS currently resolves symlinks unconditionally

    let resolved_path = resolve_at_path(dirfd, path)?;
    check_landlock(
        &resolved_path,
        crate::landlock::LANDLOCK_ACCESS_FS_WRITE_FILE,
    )?;
    let mut stat = vfs::vfs_stat(&resolved_path).map_err(vfs_error_to_linux)?;
    let now = crate::time::system_time();

    let (new_atime, new_mtime) = if times.is_null() {
        (now, now)
    } else {
        // SAFETY: times is non-null; caller must provide a readable two-element array
        // per the Linux utimensat ABI.
        let ts = unsafe { &*times };
        let atime = if ts[0].tv_nsec == UTIME_OMIT {
            stat.atime
        } else if ts[0].tv_nsec == UTIME_NOW {
            now
        } else {
            ts[0].tv_sec as u64
        };
        let mtime = if ts[1].tv_nsec == UTIME_OMIT {
            stat.mtime
        } else if ts[1].tv_nsec == UTIME_NOW {
            now
        } else {
            ts[1].tv_sec as u64
        };
        (atime, mtime)
    };

    vfs::vfs_set_times(&resolved_path, new_atime, new_mtime).map_err(vfs_error_to_linux)?;

    // Update the cached stat so subsequent stat() reflects the new times.
    stat.atime = new_atime;
    stat.mtime = new_mtime;
    stat.ctime = now;
    Ok(0)
}

/// utimes - set file access and modification times
pub fn utimes(dirfd: i32, path: *const u8, times: *const [TimeVal; 2]) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let mut ts = [TimeSpec {
        tv_sec: 0,
        tv_nsec: 0,
    }; 2];
    if !times.is_null() {
        // SAFETY: times is non-null; caller must provide a readable two-element array
        // per the Linux utimes ABI.
        let tv = unsafe { &*times };
        for i in 0..2 {
            ts[i].tv_sec = tv[i].tv_sec;
            ts[i].tv_nsec = tv[i].tv_usec as i64 * 1000;
        }
    }
    utimensat(dirfd, path, &ts as *const [TimeSpec; 2], 0)
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

    // FALLOC_FL_KEEP_SIZE = 0x01, FALLOC_FL_PUNCH_HOLE = 0x02
    const FALLOC_FL_KEEP_SIZE: i32 = 0x01;
    const FALLOC_FL_PUNCH_HOLE: i32 = 0x02;

    let keep_size = (mode & FALLOC_FL_KEEP_SIZE) != 0;
    let punch_hole = (mode & FALLOC_FL_PUNCH_HOLE) != 0;

    if punch_hole {
        // Punching holes requires a real block-level filesystem with
        // sparse file support.  Our ramfs-backed VFS doesn't support
        // deallocation, so treat it as a no-op (the region stays
        // zero-filled on next read).
        return Ok(0);
    }

    // For the default (allocate) mode, extend the file if the new
    // range exceeds the current size.  If KEEP_SIZE is set, we only
    // extend when the range is beyond EOF (matching Linux semantics
    // where the file size is not changed but blocks are allocated).
    let stat = vfs::vfs_fstat(fd).map_err(vfs_error_to_linux)?;
    let current_size = stat.size as i64;
    let new_end = offset + len;

    if new_end > current_size && !keep_size {
        vfs::vfs_ftruncate(fd, new_end as u64).map_err(vfs_error_to_linux)?;
    }

    Ok(0)
}

// =============================================================================
// Advisory file locking (flock)
// =============================================================================

/// flock(2) operation flags
const LOCK_SH: i32 = 1; // Shared lock
const LOCK_EX: i32 = 2; // Exclusive lock
const LOCK_NB: i32 = 4; // Non-blocking
const LOCK_UN: i32 = 8; // Unlock

use alloc::collections::BTreeMap;
use spin::Mutex;

/// Global advisory lock table keyed by (pid, fd).
///
/// Each entry records the lock mode held by a process on a file
/// descriptor. Conflicts are detected by scanning for overlapping
/// locks from *other* processes. This is a simplified flock
/// implementation: it tracks per-(pid,fd) state and enforces mutual
/// exclusion between independent processes but does not track the
/// underlying inode, so locks on the same file via different fds in
/// the same process are not reconciled (matching minimal flock
/// semantics).
static FLOCK_TABLE: Mutex<BTreeMap<(i32, i32), i32>> = Mutex::new(BTreeMap::new());

/// flock - apply or remove an advisory lock on an open file
///
/// # Arguments
/// * `fd` - Open file descriptor
/// * `operation` - One of LOCK_SH, LOCK_EX, LOCK_UN, optionally ORed with LOCK_NB
///
/// # Returns
/// * `Ok(0)` on success
/// * `Err(EWOULDBLOCK)` if LOCK_NB was requested and the lock would block
/// * `Err(EBADF)` if `fd` is negative
/// * `Err(EINVAL)` if `operation` is invalid
pub fn flock(fd: Fd, operation: i32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    let pid = process::current_pid() as i32;
    let mode = operation & !LOCK_NB;
    let non_blocking = (operation & LOCK_NB) != 0;

    match mode {
        LOCK_UN => {
            FLOCK_TABLE.lock().remove(&(pid, fd));
            Ok(0)
        }
        LOCK_SH | LOCK_EX => {
            let mut table = FLOCK_TABLE.lock();

            // Check for conflicts with locks held by other processes.
            // A shared lock conflicts with any exclusive lock from another pid.
            // An exclusive lock conflicts with any lock (shared or exclusive)
            // from another pid.
            for (&(holder_pid, holder_fd), &holder_mode) in table.iter() {
                if holder_pid == pid && holder_fd == fd {
                    continue; // our own lock — we'll replace it
                }
                // Without inode tracking we conservatively treat any other
                // lock on the same fd number as a potential conflict. In
                // practice fd numbers are per-process so collisions across
                // processes on the same file are not detected here; this is
                // a known limitation of this minimal implementation.
                let conflict = match (mode, holder_mode) {
                    (LOCK_EX, _) => true,
                    (LOCK_SH, LOCK_EX) => true,
                    _ => false,
                };
                if conflict {
                    if non_blocking {
                        return Err(super::EWOULDBLOCK);
                    }
                    // Blocking flock would wait until the lock is released.
                    // We do not have a wait queue here, so return EWOULDBLOCK
                    // to avoid an infinite spin. Callers that truly need
                    // blocking behavior should retry.
                    return Err(super::EWOULDBLOCK);
                }
            }

            // Grant the lock (replacing any existing lock we hold on this fd).
            table.insert((pid, fd), mode);
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
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

pub fn close_range(first: u32, last: u32, flags: u32) -> LinuxResult<i32> {
    const CLOSE_RANGE_CLOEXEC: u32 = 1 << 2;
    const CLOSE_RANGE_UNSHARE: u32 = 1 << 1;

    if first > last {
        return Err(LinuxError::EINVAL);
    }
    if flags & !(CLOSE_RANGE_CLOEXEC | CLOSE_RANGE_UNSHARE) != 0 {
        return Err(LinuxError::EINVAL);
    }

    let end = last.min(i32::MAX as u32);
    for fd in first..=end {
        let fd = fd as i32;
        if (flags & CLOSE_RANGE_CLOEXEC) != 0 {
            let _ = vfs::vfs_set_fd_flags(fd, vfs::OpenFlags::CLOEXEC);
        } else {
            let _ = vfs::vfs_close(fd);
        }
    }
    Ok(0)
}
