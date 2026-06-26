//! Linux ioctl and fcntl operations
//!
//! This module implements device control and file control operations
//! including ioctl, fcntl, and related system calls.

#![no_std]

extern crate alloc;

use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxError, LinuxResult};

/// Operation counter for statistics
static IOCTL_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize ioctl operations subsystem
pub fn init_ioctl_operations() {
    IOCTL_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of ioctl operations performed
pub fn get_operation_count() -> u64 {
    IOCTL_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    IOCTL_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
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
}

/// fcntl - file control operations
pub fn fcntl(fd: Fd, cmd: i32, arg: u64) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    match cmd {
        fcntl_cmd::F_DUPFD => {
            // TODO: Duplicate file descriptor with minimum value
            Ok(fd + 1)
        }
        fcntl_cmd::F_DUPFD_CLOEXEC => {
            // TODO: Duplicate fd with close-on-exec flag set
            Ok(fd + 1)
        }
        fcntl_cmd::F_GETFD => {
            // TODO: Get file descriptor flags
            Ok(0)
        }
        fcntl_cmd::F_SETFD => {
            // TODO: Set file descriptor flags
            let flags = arg as i32;
            if flags & !fcntl_flags::FD_CLOEXEC != 0 {
                return Err(LinuxError::EINVAL);
            }
            Ok(0)
        }
        fcntl_cmd::F_GETFL => {
            // TODO: Get file status flags
            Ok(open_flags::O_RDWR as i32)
        }
        fcntl_cmd::F_SETFL => {
            // TODO: Set file status flags
            // Only certain flags can be changed (O_APPEND, O_NONBLOCK, etc.)
            Ok(0)
        }
        fcntl_cmd::F_GETLK => {
            // TODO: Get record lock info
            if arg == 0 {
                return Err(LinuxError::EFAULT);
            }
            Ok(0)
        }
        fcntl_cmd::F_SETLK | fcntl_cmd::F_SETLKW => {
            // TODO: Set record lock
            if arg == 0 {
                return Err(LinuxError::EFAULT);
            }
            Ok(0)
        }
        fcntl_cmd::F_GETOWN => {
            // TODO: Get process/group for SIGIO
            Ok(0)
        }
        fcntl_cmd::F_SETOWN => {
            // TODO: Set process/group for SIGIO
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// ioctl - device control operations
pub fn ioctl(fd: Fd, request: u64, argp: u64) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    match request {
        // Terminal control operations
        ioctl_req::TCGETS => {
            // TODO: Get terminal attributes
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            Ok(0)
        }
        ioctl_req::TCSETS | ioctl_req::TCSETSW | ioctl_req::TCSETSF => {
            // TODO: Set terminal attributes
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            Ok(0)
        }
        ioctl_req::TIOCGWINSZ => {
            // TODO: Get window size
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            // Default window size
            unsafe {
                let winsize = argp as *mut WinSize;
                (*winsize).ws_row = 24;
                (*winsize).ws_col = 80;
                (*winsize).ws_xpixel = 0;
                (*winsize).ws_ypixel = 0;
            }
            Ok(0)
        }
        ioctl_req::TIOCSWINSZ => {
            // TODO: Set window size
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            Ok(0)
        }
        ioctl_req::TCFLSH => {
            // TODO: Flush terminal I/O
            Ok(0)
        }
        ioctl_req::TIOCGPGRP => {
            // TODO: Get foreground process group
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            Ok(0)
        }
        ioctl_req::TIOCSPGRP => {
            // TODO: Set foreground process group
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            Ok(0)
        }
        _ => {
            // Unknown ioctl request
            Err(LinuxError::ENOTTY)
        }
    }
}

/// flock - apply or remove an advisory lock on a file
pub fn flock(fd: Fd, operation: i32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // Lock operations
    const LOCK_SH: i32 = 1; // Shared lock
    const LOCK_EX: i32 = 2; // Exclusive lock
    const LOCK_UN: i32 = 8; // Unlock
    const LOCK_NB: i32 = 4; // Non-blocking

    let op = operation & !LOCK_NB;
    match op {
        LOCK_SH | LOCK_EX | LOCK_UN => {
            // TODO: Implement file locking
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// Window size structure
#[repr(C)]
pub struct WinSize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
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
