//! Linux Compatibility Layer
//!
//! This module provides a comprehensive Linux/POSIX API compatibility layer
//! for RustOS, enabling Linux applications to run with minimal modifications.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub mod advanced_io;
pub mod file_ops;
pub mod fs_ops;
pub mod ioctl_ops;
pub mod ipc_ops;
pub mod memory_ops;
pub mod process_ops;
pub mod resource_ops;
pub mod signal_ops;
pub mod socket_ops;
pub mod sysinfo_ops;
pub mod thread_ops;
pub mod time_ops;
pub mod tty_ops;
pub mod types;

pub use types::*;

/// Linux API compatibility result type
pub type LinuxResult<T> = Result<T, LinuxError>;

/// Linux error codes (matching errno values)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum LinuxError {
    /// Operation not permitted
    EPERM = 1,
    /// No such file or directory
    ENOENT = 2,
    /// No such process
    ESRCH = 3,
    /// Interrupted system call
    EINTR = 4,
    /// I/O error
    EIO = 5,
    /// No such device or address
    ENXIO = 6,
    /// Argument list too long
    E2BIG = 7,
    /// Exec format error
    ENOEXEC = 8,
    /// Bad file number
    EBADF = 9,
    /// No child processes
    ECHILD = 10,
    /// Try again
    EAGAIN = 11,
    /// Out of memory
    ENOMEM = 12,
    /// Permission denied
    EACCES = 13,
    /// Bad address
    EFAULT = 14,
    /// Block device required
    ENOTBLK = 15,
    /// Device or resource busy
    EBUSY = 16,
    /// File exists
    EEXIST = 17,
    /// Cross-device link
    EXDEV = 18,
    /// No such device
    ENODEV = 19,
    /// Not a directory
    ENOTDIR = 20,
    /// Is a directory
    EISDIR = 21,
    /// Invalid argument
    EINVAL = 22,
    /// File table overflow
    ENFILE = 23,
    /// Too many open files
    EMFILE = 24,
    /// Not a typewriter
    ENOTTY = 25,
    /// Text file busy
    ETXTBSY = 26,
    /// File too large
    EFBIG = 27,
    /// No space left on device
    ENOSPC = 28,
    /// Illegal seek
    ESPIPE = 29,
    /// Read-only file system
    EROFS = 30,
    /// Too many links
    EMLINK = 31,
    /// Broken pipe
    EPIPE = 32,
    /// Math argument out of domain
    EDOM = 33,
    /// Math result not representable
    ERANGE = 34,
    /// Resource deadlock would occur
    EDEADLK = 35,
    /// File name too long
    ENAMETOOLONG = 36,
    /// No record locks available
    ENOLCK = 37,
    /// Function not implemented
    ENOSYS = 38,
    /// Directory not empty
    ENOTEMPTY = 39,
    /// Too many symbolic links encountered
    ELOOP = 40,
    /// No message of desired type
    ENOMSG = 42,
    /// Identifier removed
    EIDRM = 43,
    /// No data available
    ENODATA = 61,
    /// Not supported
    ENOTSUP = 95,
}

// Linux compatibility aliases - these errno values are intentionally the same
/// Operation would block (alias for EAGAIN)
pub const EWOULDBLOCK: LinuxError = LinuxError::EAGAIN;
/// Operation not supported on transport endpoint (alias for ENOTSUP)
pub const EOPNOTSUPP: LinuxError = LinuxError::ENOTSUP;

impl LinuxError {
    /// Convert to errno value
    pub fn to_errno(self) -> i32 {
        self as i32
    }

    /// Convert from errno value
    pub fn from_errno(errno: i32) -> Self {
        match errno {
            1 => LinuxError::EPERM,
            2 => LinuxError::ENOENT,
            3 => LinuxError::ESRCH,
            4 => LinuxError::EINTR,
            5 => LinuxError::EIO,
            9 => LinuxError::EBADF,
            10 => LinuxError::ECHILD,
            11 => LinuxError::EAGAIN,
            12 => LinuxError::ENOMEM,
            13 => LinuxError::EACCES,
            14 => LinuxError::EFAULT,
            16 => LinuxError::EBUSY,
            17 => LinuxError::EEXIST,
            20 => LinuxError::ENOTDIR,
            21 => LinuxError::EISDIR,
            22 => LinuxError::EINVAL,
            38 => LinuxError::ENOSYS,
            _ => LinuxError::EINVAL,
        }
    }
}

/// Initialize Linux compatibility layer
pub fn init_linux_compat() {
    // Initialize subsystems
    file_ops::init_file_operations();
    process_ops::init_process_operations();
    time_ops::init_time_operations();
    signal_ops::init_signal_operations();
    socket_ops::init_socket_operations();
    ipc_ops::init_ipc_operations();
    ioctl_ops::init_ioctl_operations();
    advanced_io::init_advanced_io();
    tty_ops::init_tty_operations();
    memory_ops::init_memory_operations();
    thread_ops::init_thread_operations();
    fs_ops::init_fs_operations();
    resource_ops::init_resource_operations();
    sysinfo_ops::init_sysinfo_operations();
}

/// Get Linux compatibility layer statistics
pub fn get_compat_stats() -> CompatStats {
    CompatStats {
        file_ops_count: file_ops::get_operation_count(),
        process_ops_count: process_ops::get_operation_count(),
        time_ops_count: time_ops::get_operation_count(),
        signal_ops_count: signal_ops::get_operation_count(),
        socket_ops_count: socket_ops::get_operation_count(),
        ipc_ops_count: ipc_ops::get_operation_count(),
        ioctl_ops_count: ioctl_ops::get_operation_count(),
        advanced_io_count: advanced_io::get_operation_count(),
        tty_ops_count: tty_ops::get_operation_count(),
        memory_ops_count: memory_ops::get_operation_count(),
        thread_ops_count: thread_ops::get_operation_count(),
        fs_ops_count: fs_ops::get_operation_count(),
        resource_ops_count: resource_ops::get_operation_count(),
        sysinfo_ops_count: sysinfo_ops::get_operation_count(),
    }
}

/// Compatibility layer statistics
#[derive(Debug, Clone, Copy)]
pub struct CompatStats {
    pub file_ops_count: u64,
    pub process_ops_count: u64,
    pub time_ops_count: u64,
    pub signal_ops_count: u64,
    pub socket_ops_count: u64,
    pub ipc_ops_count: u64,
    pub ioctl_ops_count: u64,
    pub advanced_io_count: u64,
    pub tty_ops_count: u64,
    pub memory_ops_count: u64,
    pub thread_ops_count: u64,
    pub fs_ops_count: u64,
    pub resource_ops_count: u64,
    pub sysinfo_ops_count: u64,
}
