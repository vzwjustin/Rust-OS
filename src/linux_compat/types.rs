//! Linux-compatible type definitions
//!
//! This module provides data structures that match Linux kernel types
//! for binary compatibility with Linux applications.


/// File descriptor type
pub type Fd = i32;

/// Process ID type
pub type Pid = i32;

/// User ID type
pub type Uid = u32;

/// Group ID type
pub type Gid = u32;

/// File mode/permissions type
pub type Mode = u32;

/// Device number type
pub type Dev = u64;

/// Inode number type
pub type Ino = u64;

/// Number of hard links
pub type Nlink = u64;

/// File offset type
pub type Off = i64;

/// Block size type
pub type Blksize = i64;

/// Block count type
pub type Blkcnt = i64;

/// Time value (seconds since epoch)
pub type Time = i64;

/// Nanoseconds
pub type Nsec = i64;

/// File status structure (like Linux struct stat)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Stat {
    /// Device ID
    pub st_dev: Dev,
    /// Inode number
    pub st_ino: Ino,
    /// Number of hard links
    pub st_nlink: Nlink,
    /// File mode (permissions and type)
    pub st_mode: Mode,
    /// User ID of owner
    pub st_uid: Uid,
    /// Group ID of owner
    pub st_gid: Gid,
    /// Padding
    pub __pad0: u32,
    /// Device ID (if special file)
    pub st_rdev: Dev,
    /// Total size in bytes
    pub st_size: Off,
    /// Block size for I/O
    pub st_blksize: Blksize,
    /// Number of 512B blocks allocated
    pub st_blocks: Blkcnt,
    /// Time of last access
    pub st_atime: Time,
    /// Nanoseconds for last access
    pub st_atime_nsec: Nsec,
    /// Time of last modification
    pub st_mtime: Time,
    /// Nanoseconds for last modification
    pub st_mtime_nsec: Nsec,
    /// Time of last status change
    pub st_ctime: Time,
    /// Nanoseconds for last status change
    pub st_ctime_nsec: Nsec,
    /// Reserved
    pub __unused: [i64; 3],
}

impl Stat {
    pub const fn new() -> Self {
        Self {
            st_dev: 0,
            st_ino: 0,
            st_nlink: 0,
            st_mode: 0,
            st_uid: 0,
            st_gid: 0,
            __pad0: 0,
            st_rdev: 0,
            st_size: 0,
            st_blksize: 4096,
            st_blocks: 0,
            st_atime: 0,
            st_atime_nsec: 0,
            st_mtime: 0,
            st_mtime_nsec: 0,
            st_ctime: 0,
            st_ctime_nsec: 0,
            __unused: [0; 3],
        }
    }
}

/// Time specification (like Linux struct timespec)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeSpec {
    /// Seconds
    pub tv_sec: Time,
    /// Nanoseconds
    pub tv_nsec: Nsec,
}

impl TimeSpec {
    pub const fn new(sec: Time, nsec: Nsec) -> Self {
        Self {
            tv_sec: sec,
            tv_nsec: nsec,
        }
    }

    pub const fn zero() -> Self {
        Self {
            tv_sec: 0,
            tv_nsec: 0,
        }
    }
}

/// Time value (like Linux struct timeval)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TimeVal {
    /// Seconds
    pub tv_sec: Time,
    /// Microseconds
    pub tv_usec: i64,
}

/// Resource usage statistics (like Linux struct rusage)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Rusage {
    /// User CPU time
    pub ru_utime: TimeVal,
    /// System CPU time
    pub ru_stime: TimeVal,
    /// Maximum resident set size
    pub ru_maxrss: i64,
    /// Integral shared memory size
    pub ru_ixrss: i64,
    /// Integral unshared data size
    pub ru_idrss: i64,
    /// Integral unshared stack size
    pub ru_isrss: i64,
    /// Page reclaims (soft page faults)
    pub ru_minflt: i64,
    /// Page faults (hard page faults)
    pub ru_majflt: i64,
    /// Swaps
    pub ru_nswap: i64,
    /// Block input operations
    pub ru_inblock: i64,
    /// Block output operations
    pub ru_oublock: i64,
    /// IPC messages sent
    pub ru_msgsnd: i64,
    /// IPC messages received
    pub ru_msgrcv: i64,
    /// Signals received
    pub ru_nsignals: i64,
    /// Voluntary context switches
    pub ru_nvcsw: i64,
    /// Involuntary context switches
    pub ru_nivcsw: i64,
}

/// Signal action structure (like Linux struct sigaction)
#[repr(C)]
pub struct SigAction {
    /// Signal handler
    pub sa_handler: usize,
    /// Signal flags
    pub sa_flags: u32,
    /// Restorer function
    pub sa_restorer: usize,
    /// Signal mask
    pub sa_mask: SigSet,
}

/// Signal set type
pub type SigSet = u64;

/// Socket address structure (generic)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SockAddr {
    /// Address family
    pub sa_family: u16,
    /// Address data (14 bytes)
    pub sa_data: [u8; 14],
}

/// Socket address storage (large enough for any address type)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SockAddrStorage {
    /// Address family
    pub ss_family: u16,
    /// Padding/alignment
    pub __ss_align: u64,
    /// Storage (112 bytes)
    pub __ss_padding: [u8; 112],
}

/// I/O vector for scatter/gather I/O
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IoVec {
    /// Base address
    pub iov_base: *mut u8,
    /// Length
    pub iov_len: usize,
}

/// Poll file descriptor structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PollFd {
    /// File descriptor
    pub fd: Fd,
    /// Requested events
    pub events: i16,
    /// Returned events
    pub revents: i16,
}

/// Directory entry structure
#[repr(C)]
pub struct Dirent {
    /// Inode number
    pub d_ino: Ino,
    /// Offset to next dirent
    pub d_off: Off,
    /// Length of this dirent
    pub d_reclen: u16,
    /// File type
    pub d_type: u8,
    /// File name
    pub d_name: [u8; 256],
}

/// File mode constants
pub mod mode {
    use super::Mode;

    /// File type mask
    pub const S_IFMT: Mode = 0o170000;
    /// Socket
    pub const S_IFSOCK: Mode = 0o140000;
    /// Symbolic link
    pub const S_IFLNK: Mode = 0o120000;
    /// Regular file
    pub const S_IFREG: Mode = 0o100000;
    /// Block device
    pub const S_IFBLK: Mode = 0o060000;
    /// Directory
    pub const S_IFDIR: Mode = 0o040000;
    /// Character device
    pub const S_IFCHR: Mode = 0o020000;
    /// FIFO
    pub const S_IFIFO: Mode = 0o010000;

    /// Set-user-ID
    pub const S_ISUID: Mode = 0o4000;
    /// Set-group-ID
    pub const S_ISGID: Mode = 0o2000;
    /// Sticky bit
    pub const S_ISVTX: Mode = 0o1000;

    /// User read
    pub const S_IRUSR: Mode = 0o400;
    /// User write
    pub const S_IWUSR: Mode = 0o200;
    /// User execute
    pub const S_IXUSR: Mode = 0o100;

    /// Group read
    pub const S_IRGRP: Mode = 0o040;
    /// Group write
    pub const S_IWGRP: Mode = 0o020;
    /// Group execute
    pub const S_IXGRP: Mode = 0o010;

    /// Other read
    pub const S_IROTH: Mode = 0o004;
    /// Other write
    pub const S_IWOTH: Mode = 0o002;
    /// Other execute
    pub const S_IXOTH: Mode = 0o001;
}

/// Open flags (matching Linux)
pub mod open_flags {
    /// Read only
    pub const O_RDONLY: i32 = 0o0;
    /// Write only
    pub const O_WRONLY: i32 = 0o1;
    /// Read and write
    pub const O_RDWR: i32 = 0o2;
    /// Create if not exists
    pub const O_CREAT: i32 = 0o100;
    /// Exclusive create
    pub const O_EXCL: i32 = 0o200;
    /// Don't assign controlling terminal
    pub const O_NOCTTY: i32 = 0o400;
    /// Truncate
    pub const O_TRUNC: i32 = 0o1000;
    /// Append
    pub const O_APPEND: i32 = 0o2000;
    /// Non-blocking
    pub const O_NONBLOCK: i32 = 0o4000;
    /// Directory
    pub const O_DIRECTORY: i32 = 0o200000;
    /// No follow symlinks
    pub const O_NOFOLLOW: i32 = 0o400000;
    /// Close-on-exec
    pub const O_CLOEXEC: i32 = 0o2000000;
}

/// Access mode constants
pub mod access {
    /// Test for existence
    pub const F_OK: i32 = 0;
    /// Test for execute permission
    pub const X_OK: i32 = 1;
    /// Test for write permission
    pub const W_OK: i32 = 2;
    /// Test for read permission
    pub const R_OK: i32 = 4;
}

/// Clock IDs
pub mod clock {
    /// System-wide realtime clock
    pub const CLOCK_REALTIME: i32 = 0;
    /// Monotonic clock
    pub const CLOCK_MONOTONIC: i32 = 1;
    /// High-resolution per-process timer
    pub const CLOCK_PROCESS_CPUTIME_ID: i32 = 2;
    /// Thread-specific CPU-time clock
    pub const CLOCK_THREAD_CPUTIME_ID: i32 = 3;
    /// Monotonic raw hardware based time
    pub const CLOCK_MONOTONIC_RAW: i32 = 4;
    /// Boot time clock
    pub const CLOCK_BOOTTIME: i32 = 7;
}

/// Signal numbers (Linux x86-64)
pub mod signal {
    /// Hangup
    pub const SIGHUP: i32 = 1;
    /// Interrupt
    pub const SIGINT: i32 = 2;
    /// Quit
    pub const SIGQUIT: i32 = 3;
    /// Illegal instruction
    pub const SIGILL: i32 = 4;
    /// Trace trap
    pub const SIGTRAP: i32 = 5;
    /// Abort
    pub const SIGABRT: i32 = 6;
    /// Bus error
    pub const SIGBUS: i32 = 7;
    /// Floating point exception
    pub const SIGFPE: i32 = 8;
    /// Kill
    pub const SIGKILL: i32 = 9;
    /// User defined signal 1
    pub const SIGUSR1: i32 = 10;
    /// Segmentation fault
    pub const SIGSEGV: i32 = 11;
    /// User defined signal 2
    pub const SIGUSR2: i32 = 12;
    /// Pipe broken
    pub const SIGPIPE: i32 = 13;
    /// Alarm clock
    pub const SIGALRM: i32 = 14;
    /// Termination
    pub const SIGTERM: i32 = 15;
    /// Child status changed
    pub const SIGCHLD: i32 = 17;
    /// Continue
    pub const SIGCONT: i32 = 18;
    /// Stop
    pub const SIGSTOP: i32 = 19;
    /// Keyboard stop
    pub const SIGTSTP: i32 = 20;
}

/// Arguments for clone3 system call
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CloneArgs {
    /// Flags bit mask
    pub flags: u64,
    /// Where to store pidfd
    pub pidfd: u64,
    /// Where to store child TID in child's memory
    pub child_tid: u64,
    /// Where to store child TID in parent's memory
    pub parent_tid: u64,
    /// Signal to deliver to parent on child exit
    pub exit_signal: u64,
    /// Pointer to lowest address of stack
    pub stack: u64,
    /// Size of stack
    pub stack_size: u64,
    /// Location of new TLS
    pub tls: u64,
    /// Pointer to an array of preferred TIDs
    pub set_tid: u64,
    /// Size of set_tid array
    pub set_tid_size: u64,
    /// File descriptor for target cgroup of child
    pub cgroup: u64,
}

/// Timestamp representation for statx
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct StatxTimestamp {
    /// Seconds since Epoch
    pub tv_sec: i64,
    /// Nanoseconds
    pub tv_nsec: u32,
    /// Reserved for alignment/future
    pub __reserved: i32,
}

/// Detailed file status structure for statx system call
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Statx {
    /// Mask indicating which fields are populated
    pub stx_mask: u32,
    /// Optimal block size for I/O
    pub stx_blksize: u32,
    /// Extra file attributes
    pub stx_attributes: u64,
    /// Number of hard links
    pub stx_nlink: u32,
    /// User ID of owner
    pub stx_uid: u32,
    /// Group ID of owner
    pub stx_gid: u32,
    /// File type and mode
    pub stx_mode: u16,
    /// Spare padding
    pub __spare0: [u16; 1],
    /// Inode number
    pub stx_ino: u64,
    /// Total size in bytes
    pub stx_size: u64,
    /// Number of 512B blocks allocated
    pub stx_blocks: u64,
    /// Mask showing which attributes are supported
    pub stx_attributes_mask: u64,
    /// Last access time
    pub stx_atime: StatxTimestamp,
    /// Creation (birth) time
    pub stx_btime: StatxTimestamp,
    /// Last status change time
    pub stx_ctime: StatxTimestamp,
    /// Last modification time
    pub stx_mtime: StatxTimestamp,
    /// Device major ID (if device file)
    pub stx_rdev_major: u32,
    /// Device minor ID (if device file)
    pub stx_rdev_minor: u32,
    /// Device major ID containing file
    pub stx_dev_major: u32,
    /// Device minor ID containing file
    pub stx_dev_minor: u32,
    /// Padding/reserved for future use
    pub __spare2: [u64; 14],
}

/// Arguments for openat2 system call
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct OpenHow {
    /// Open flags (O_* constants)
    pub flags: u64,
    /// Mode for file creation
    pub mode: u64,
    /// Resolve flags (RESOLVE_* constants)
    pub resolve: u64,
}

