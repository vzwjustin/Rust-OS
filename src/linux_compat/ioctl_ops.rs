//! Linux ioctl and fcntl operations
//!
//! This module implements device control and file control operations
//! including ioctl, fcntl, and related system calls.

extern crate alloc;

use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

use super::file_ops;
use super::tty_ops::{self, Termios, WinSize};
use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::process;
use crate::vfs;

/// Operation counter for statistics
static IOCTL_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Per-process fd metadata for fcntl.
static FD_META: RwLock<BTreeMap<(u32, i32), FdMeta>> = RwLock::new(BTreeMap::new());

/// Advisory flock table keyed by (pid, fd).
static FLOCK_TABLE: RwLock<BTreeMap<(u32, i32), i32>> = RwLock::new(BTreeMap::new());

#[derive(Clone, Copy, Default)]
struct FdMeta {
    cloexec: bool,
    status_flags: i32,
}

/// Initialize ioctl operations subsystem
pub fn init_ioctl_operations() {
    IOCTL_OPS_COUNT.store(0, Ordering::Relaxed);
    FD_META.write().clear();
    FLOCK_TABLE.write().clear();
}

/// Get number of ioctl operations performed
pub fn get_operation_count() -> u64 {
    IOCTL_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    IOCTL_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

fn current_pid() -> u32 {
    process::current_pid()
}

fn validate_fd(fd: Fd) -> LinuxResult<()> {
    if fd < 0 {
        return Err(LinuxError::EBADF);
    }
    if tty_ops::is_registered_tty_fd(fd) {
        return Ok(());
    }
    if vfs::vfs_fstat(fd).is_ok() {
        return Ok(());
    }
    Err(LinuxError::EBADF)
}

fn fd_meta(pid: u32, fd: i32) -> FdMeta {
    FD_META.read().get(&(pid, fd)).copied().unwrap_or_default()
}

fn set_fd_meta(pid: u32, fd: i32, meta: FdMeta) {
    FD_META.write().insert((pid, fd), meta);
}

fn copy_fd_meta(pid: u32, oldfd: i32, newfd: i32) {
    let meta = fd_meta(pid, oldfd);
    set_fd_meta(pid, newfd, meta);
}

fn dup_to_fd(oldfd: Fd, newfd: i32) -> LinuxResult<i32> {
    if tty_ops::is_pty_fd(oldfd) || (oldfd >= 0 && oldfd <= 2) {
        tty_ops::dup_tty_fd(oldfd, newfd)
    } else {
        file_ops::dup2(oldfd, newfd)
    }
}

fn default_status_flags(fd: Fd) -> i32 {
    if tty_ops::is_registered_tty_fd(fd) {
        return open_flags::O_RDWR;
    }
    if let Ok(stat) = vfs::vfs_fstat(fd) {
        let access = stat.mode & 0o3;
        return match access {
            0o1 => open_flags::O_WRONLY,
            0o2 => open_flags::O_RDWR,
            _ => open_flags::O_RDONLY,
        } as i32;
    }
    open_flags::O_RDWR
}

fn fd_in_use(pid: u32, candidate: i32) -> bool {
    if tty_ops::is_registered_tty_fd(candidate) {
        return true;
    }
    if vfs::vfs_fstat(candidate).is_ok() {
        return true;
    }
    FD_META.read().contains_key(&(pid, candidate))
}

fn dupfd_min(oldfd: Fd, minfd: i32, cloexec: bool) -> LinuxResult<i32> {
    validate_fd(oldfd)?;

    if minfd < 0 {
        return Err(LinuxError::EINVAL);
    }

    let pid = current_pid();
    let mut candidate = minfd;
    while candidate < 1024 {
        if !fd_in_use(pid, candidate) {
            break;
        }
        candidate += 1;
    }

    if candidate >= 1024 {
        return Err(LinuxError::EMFILE);
    }

    let newfd = dup_to_fd(oldfd, candidate)?;

    copy_fd_meta(pid, oldfd, newfd);
    let mut meta = fd_meta(pid, newfd);
    meta.cloexec = cloexec;
    set_fd_meta(pid, newfd, meta);
    Ok(newfd)
}

// fcntl command constants
pub mod fcntl_cmd {
    /// Duplicate file descriptor
    pub const F_DUPFD: i32 = 0;
    /// Duplicate file descriptor with close-on-exec
    pub const F_DUPFD_CLOEXEC: i32 = 1030;
    /// Get file descriptor flags
    pub const F_GETFD: i32 = 1;
    /// Set file descriptor flags
    pub const F_SETFD: i32 = 2;
    /// Get file status flags
    pub const F_GETFL: i32 = 3;
    /// Set file status flags
    pub const F_SETFL: i32 = 4;
    /// Get record locking info
    pub const F_GETLK: i32 = 5;
    /// Set record locking info
    pub const F_SETLK: i32 = 6;
    /// Set record locking info (blocking)
    pub const F_SETLKW: i32 = 7;
    /// Get owner for SIGIO
    pub const F_GETOWN: i32 = 9;
    /// Set owner for SIGIO
    pub const F_SETOWN: i32 = 8;
}

// fcntl flags
pub mod fcntl_flags {
    /// Close-on-exec flag
    pub const FD_CLOEXEC: i32 = 1;
}

// ioctl request types
pub mod ioctl_req {
    /// Terminal I/O
    pub const TCGETS: u64 = 0x5401;
    pub const TCSETS: u64 = 0x5402;
    pub const TCSETSW: u64 = 0x5403;
    pub const TCSETSF: u64 = 0x5404;
    pub const TCGETA: u64 = 0x5405;
    pub const TCSETA: u64 = 0x5406;
    pub const TCSETAW: u64 = 0x5407;
    pub const TCSETAF: u64 = 0x5408;

    /// Window size
    pub const TIOCGWINSZ: u64 = 0x5413;
    pub const TIOCSWINSZ: u64 = 0x5414;

    /// Flushing
    pub const TCFLSH: u64 = 0x540B;

    /// Get/set foreground process group
    pub const TIOCGPGRP: u64 = 0x540F;
    pub const TIOCSPGRP: u64 = 0x5410;

    /// Bytes available to read
    pub const FIONREAD: u64 = 0x541B;
}

/// fcntl - file control operations
pub fn fcntl(fd: Fd, cmd: i32, arg: u64) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    let pid = current_pid();

    match cmd {
        fcntl_cmd::F_DUPFD => dupfd_min(fd, arg as i32, false),
        fcntl_cmd::F_DUPFD_CLOEXEC => dupfd_min(fd, arg as i32, true),
        fcntl_cmd::F_GETFD => {
            validate_fd(fd)?;
            let meta = fd_meta(pid, fd);
            Ok(if meta.cloexec {
                fcntl_flags::FD_CLOEXEC
            } else {
                0
            })
        }
        fcntl_cmd::F_SETFD => {
            validate_fd(fd)?;
            let flags = arg as i32;
            if flags & !fcntl_flags::FD_CLOEXEC != 0 {
                return Err(LinuxError::EINVAL);
            }
            let mut meta = fd_meta(pid, fd);
            meta.cloexec = flags & fcntl_flags::FD_CLOEXEC != 0;
            set_fd_meta(pid, fd, meta);
            Ok(0)
        }
        fcntl_cmd::F_GETFL => {
            validate_fd(fd)?;
            let meta = fd_meta(pid, fd);
            let base = if meta.status_flags != 0 {
                meta.status_flags
            } else {
                default_status_flags(fd)
            };
            Ok(base)
        }
        fcntl_cmd::F_SETFL => {
            validate_fd(fd)?;
            let allowed = open_flags::O_APPEND | open_flags::O_NONBLOCK;
            let flags = arg as i32;
            if flags & !allowed != 0 {
                return Err(LinuxError::EINVAL);
            }
            let mut meta = fd_meta(pid, fd);
            let base = default_status_flags(fd) & !(open_flags::O_APPEND | open_flags::O_NONBLOCK);
            meta.status_flags = base | (flags & allowed);
            set_fd_meta(pid, fd, meta);
            Ok(0)
        }
        fcntl_cmd::F_GETLK => {
            if arg == 0 {
                return Err(LinuxError::EFAULT);
            }
            // Check for a conflicting lock. If none, set l_type to F_UNLCK.
            // struct flock: l_type(2), l_whence(2), l_start(8), l_len(8), l_pid(4)
            let mut fl = [0u8; 24];
            unsafe {
                core::ptr::copy_nonoverlapping(arg as *const u8, fl.as_mut_ptr(), 24);
            }
            let l_type = i16::from_ne_bytes([fl[0], fl[1]]);
            let l_start =
                i64::from_ne_bytes([fl[4], fl[5], fl[6], fl[7], fl[8], fl[9], fl[10], fl[11]]);
            let l_len = i64::from_ne_bytes([
                fl[12], fl[13], fl[14], fl[15], fl[16], fl[17], fl[18], fl[19],
            ]);

            // F_UNLCK = 2, F_RDLCK = 0, F_WRLCK = 1
            if l_type == 2 {
                return Err(LinuxError::EINVAL);
            }

            let pid = process::current_pid();
            let conflict = check_posix_lock_conflict(pid, fd, l_type, l_start, l_len);
            if let Some(conflict_pid) = conflict {
                // Report the conflicting lock
                fl[0..2].copy_from_slice(&1i16.to_ne_bytes()); // F_WRLCK
                fl[20..24].copy_from_slice(&(conflict_pid as i32).to_ne_bytes());
            } else {
                // No conflict — set F_UNLCK
                fl[0..2].copy_from_slice(&2i16.to_ne_bytes()); // F_UNLCK
                fl[20..24].copy_from_slice(&0i32.to_ne_bytes());
            }
            unsafe {
                core::ptr::copy_nonoverlapping(fl.as_ptr(), arg as *mut u8, 24);
            }
            Ok(0)
        }
        fcntl_cmd::F_SETLK | fcntl_cmd::F_SETLKW => {
            if arg == 0 {
                return Err(LinuxError::EFAULT);
            }
            let mut fl = [0u8; 24];
            unsafe {
                core::ptr::copy_nonoverlapping(arg as *const u8, fl.as_mut_ptr(), 24);
            }
            let l_type = i16::from_ne_bytes([fl[0], fl[1]]);
            let l_start =
                i64::from_ne_bytes([fl[4], fl[5], fl[6], fl[7], fl[8], fl[9], fl[10], fl[11]]);
            let l_len = i64::from_ne_bytes([
                fl[12], fl[13], fl[14], fl[15], fl[16], fl[17], fl[18], fl[19],
            ]);

            let pid = process::current_pid();
            let blocking = cmd == fcntl_cmd::F_SETLKW;

            apply_posix_lock(pid, fd, l_type, l_start, l_len, blocking)
        }
        fcntl_cmd::F_GETOWN => {
            validate_fd(fd)?;
            Ok(0)
        }
        fcntl_cmd::F_SETOWN => {
            validate_fd(fd)?;
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

// =============================================================================
// POSIX record locks (fcntl F_SETLK / F_SETLKW / F_GETLK)
// =============================================================================

/// A POSIX (advisory) record lock entry.
#[derive(Clone, Copy)]
struct PosixLock {
    pid: u32,
    fd: i32,
    l_type: i16, // 0 = F_RDLCK, 1 = F_WRLCK
    start: i64,
    len: i64, // 0 means "to end of file"
}

/// Global table of POSIX record locks.
static POSIX_LOCKS: spin::Mutex<alloc::vec::Vec<PosixLock>> =
    spin::Mutex::new(alloc::vec::Vec::new());

/// Check if a requested lock conflicts with an existing lock held by
/// another process. Returns the conflicting pid if there is a conflict.
fn check_posix_lock_conflict(pid: u32, fd: i32, l_type: i16, start: i64, len: i64) -> Option<u32> {
    let locks = POSIX_LOCKS.lock();
    for lock in locks.iter() {
        if lock.pid == pid && lock.fd == fd {
            continue; // same process, no conflict
        }
        if lock.fd != fd {
            continue; // different file
        }
        // Check byte-range overlap
        let lock_end = if lock.len == 0 {
            i64::MAX
        } else {
            lock.start + lock.len
        };
        let req_end = if len == 0 { i64::MAX } else { start + len };
        if lock.start < req_end && start < lock_end {
            // Overlapping ranges: conflict if either is exclusive
            if l_type == 1 || lock.l_type == 1 {
                return Some(lock.pid);
            }
        }
    }
    None
}

/// Apply a POSIX record lock (set, unlock, or get). For F_SETLKW,
/// retries on conflict with a short sleep.
fn apply_posix_lock(
    pid: u32,
    fd: i32,
    l_type: i16,
    start: i64,
    len: i64,
    blocking: bool,
) -> LinuxResult<i32> {
    // F_UNLCK = 2: remove matching locks
    if l_type == 2 {
        let mut locks = POSIX_LOCKS.lock();
        locks.retain(|l| !(l.pid == pid && l.fd == fd && l.start == start && l.len == len));
        return Ok(0);
    }

    // F_RDLCK = 0 or F_WRLCK = 1: set a lock
    loop {
        if let Some(conflict_pid) = check_posix_lock_conflict(pid, fd, l_type, start, len) {
            if !blocking {
                return Err(super::EWOULDBLOCK);
            }
            // Blocking: sleep and retry
            let _ = conflict_pid;
            crate::time::sleep_us(1_000);
            continue;
        }

        // Remove any existing lock from this process on this fd+range,
        // then insert the new lock.
        let mut locks = POSIX_LOCKS.lock();
        locks.retain(|l| !(l.pid == pid && l.fd == fd && l.start == start && l.len == len));
        locks.push(PosixLock {
            pid,
            fd,
            l_type,
            start,
            len,
        });
        return Ok(0);
    }
}

fn tcset_action(request: u64) -> i32 {
    match request {
        ioctl_req::TCSETS | ioctl_req::TCSETA => 0,
        ioctl_req::TCSETSW | ioctl_req::TCSETAW => 1,
        ioctl_req::TCSETSF | ioctl_req::TCSETAF => 2,
        _ => 0,
    }
}

/// ioctl - device control operations
pub fn ioctl(fd: Fd, request: u64, argp: u64) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    match request {
        ioctl_req::TCGETS | ioctl_req::TCGETA => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            tty_ops::tcgetattr(fd, argp as *mut Termios)
        }
        ioctl_req::TCSETS
        | ioctl_req::TCSETSW
        | ioctl_req::TCSETSF
        | ioctl_req::TCSETA
        | ioctl_req::TCSETAW
        | ioctl_req::TCSETAF => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            tty_ops::tcsetattr(fd, tcset_action(request), argp as *const Termios)
        }
        ioctl_req::TIOCGWINSZ => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            let winsize = tty_ops::tty_get_winsize(fd)?;
            unsafe {
                *(argp as *mut WinSize) = winsize;
            }
            Ok(0)
        }
        ioctl_req::TIOCSWINSZ => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            let winsize = unsafe { *(argp as *const WinSize) };
            tty_ops::tty_set_winsize(fd, winsize)?;
            Ok(0)
        }
        ioctl_req::TCFLSH => {
            let queue = if argp > 2 { 2 } else { argp as i32 };
            tty_ops::tcflush(fd, queue)
        }
        ioctl_req::TIOCGPGRP => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            let pgrp = tty_ops::tcgetpgrp(fd)?;
            unsafe {
                *(argp as *mut i32) = pgrp;
            }
            Ok(0)
        }
        ioctl_req::TIOCSPGRP => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            let pgrp = unsafe { *(argp as *const i32) };
            tty_ops::tcsetpgrp(fd, pgrp)
        }
        ioctl_req::FIONREAD => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            let pending = tty_ops::tty_pending_read(fd).unwrap_or(0);
            unsafe {
                *(argp as *mut i32) = pending as i32;
            }
            Ok(0)
        }
        req if crate::vfs::drmfs::is_drm_ioctl(req) => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            crate::vfs::drmfs::dispatch_ioctl_for_fd(fd, req as u32, argp)
                .map(|_| 0)
                .map_err(|_| LinuxError::ENOTTY)
        }
        _ => Err(LinuxError::ENOTTY),
    }
}

/// flock - apply or remove an advisory lock on a file
pub fn flock(fd: Fd, operation: i32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    validate_fd(fd)?;

    const LOCK_SH: i32 = 1;
    const LOCK_EX: i32 = 2;
    const LOCK_UN: i32 = 8;
    const LOCK_NB: i32 = 4;

    let pid = current_pid();
    let op = operation & !LOCK_NB;
    match op {
        LOCK_SH | LOCK_EX => {
            let key = (pid, fd);
            let mut table = FLOCK_TABLE.write();
            if let Some(current) = table.get(&key) {
                if *current == LOCK_EX && op == LOCK_SH {
                    return Err(if operation & LOCK_NB != 0 {
                        LinuxError::EAGAIN
                    } else {
                        LinuxError::EAGAIN
                    });
                }
                if *current == LOCK_EX && op == LOCK_EX {
                    return Ok(0);
                }
                if *current == LOCK_SH && op == LOCK_EX {
                    return Err(if operation & LOCK_NB != 0 {
                        LinuxError::EAGAIN
                    } else {
                        LinuxError::EAGAIN
                    });
                }
            }
            table.insert(key, op);
            Ok(0)
        }
        LOCK_UN => {
            FLOCK_TABLE.write().remove(&(pid, fd));
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}
#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_fcntl_basic() {
        assert!(fcntl(3, fcntl_cmd::F_GETFL, 0).is_ok());
        assert!(fcntl(3, fcntl_cmd::F_SETFL, open_flags::O_NONBLOCK as u64).is_ok());
        assert!(fcntl(-1, fcntl_cmd::F_GETFL, 0).is_err());
    }

    #[test_case]
    fn test_ioctl_basic() {
        let mut winsize = WinSize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        assert!(ioctl(1, ioctl_req::TIOCGWINSZ, &mut winsize as *mut _ as u64).is_ok());
        assert_eq!(winsize.ws_row, 24);
        assert_eq!(winsize.ws_col, 80);
    }
}
