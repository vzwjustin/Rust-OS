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

fn copy_value_to_user<T: Copy>(user_ptr: u64, value: &T) -> LinuxResult<()> {
    if user_ptr == 0 {
        return Err(LinuxError::EFAULT);
    }

    let bytes = super::as_bytes(value);

    crate::memory::user_space::UserSpaceMemory::copy_to_user(user_ptr, bytes)
        .map_err(|_| LinuxError::EFAULT)
}

fn copy_value_from_user<T: Copy>(user_ptr: u64) -> LinuxResult<T> {
    super::copy_struct_from_user(user_ptr as *const T)
}
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
    async_owner: i32,
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

    /// IPv4 routing table ioctls
    pub const SIOCADDRT: u64 = crate::net::routing::SIOCADDRT;
    pub const SIOCDELRT: u64 = crate::net::routing::SIOCDELRT;
    pub const SIOCRTMSG: u64 = crate::net::routing::SIOCRTMSG;
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
            crate::memory::user_space::UserSpaceMemory::copy_from_user(arg, &mut fl)
                .map_err(|_| LinuxError::EFAULT)?;
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
            crate::memory::user_space::UserSpaceMemory::copy_to_user(arg, &fl)
                .map_err(|_| LinuxError::EFAULT)?;
            Ok(0)
        }
        fcntl_cmd::F_SETLK | fcntl_cmd::F_SETLKW => {
            if arg == 0 {
                return Err(LinuxError::EFAULT);
            }
            let mut fl = [0u8; 24];
            crate::memory::user_space::UserSpaceMemory::copy_from_user(arg, &mut fl)
                .map_err(|_| LinuxError::EFAULT)?;
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
            let meta = fd_meta(pid, fd);
            Ok(meta.async_owner)
        }
        fcntl_cmd::F_SETOWN => {
            validate_fd(fd)?;
            let mut meta = fd_meta(pid, fd);
            meta.async_owner = arg as i32;
            set_fd_meta(pid, fd, meta);
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
/// Check if an ioctl request is an evdev ioctl (base 'E' = 0x45).
fn is_evdev_ioctl(req: u64) -> bool {
    ((req >> 8) & 0xFF) == 0x45
}

/// Handle evdev ioctls for /dev/input/eventN devices.
/// Mirrors Linux's drivers/input/evdev.c evdev_do_ioctl.
/// Ported from `/home/justin/Downloads/linux-master/drivers/input/evdev.c`.
fn handle_evdev_ioctl(fd: Fd, req: u64, argp: u64) -> LinuxResult<i32> {
    let nr = (req & 0xFF) as u8;
    let dir = (req >> 30) & 0x3;
    let size = ((req >> 16) & 0x3FFF) as usize;

    // Try to determine which evdev device this fd refers to.
    // Use device 0 (keyboard) as default, or device 1 (mouse)
    // if the fd's rdev minor suggests it.
    let device_idx = match crate::vfs::vfs_fstat(fd) {
        Ok(stat) => {
            let minor = (stat.rdev & 0xFF) as usize;
            if minor >= 64 && minor < 96 {
                (minor - 64).min(1)
            } else {
                0
            }
        }
        Err(_) => 0,
    };

    // ── Fixed-length commands (Linux evdev_do_ioctl switch) ──

    // EVIOCGVERSION: _IOR('E', 0x01, int)
    if nr == 0x01 && dir == 2 {
        let version: i32 = crate::drivers::input::evdev::EV_VERSION as i32;
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &version.to_ne_bytes())
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(0);
    }

    // EVIOCGID: _IOR('E', 0x02, struct input_id) — 8 bytes
    if nr == 0x02 && dir == 2 {
        let id_bytes = crate::drivers::input::evdev::with_device(device_idx, |dev| {
            crate::drivers::input::evdev::InputId {
                bustype: dev.bustype,
                vendor: dev.vendor,
                product: dev.product,
                version: dev.version,
            }
            .to_bytes()
        });
        if let Ok(id) = id_bytes {
            crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &id)
                .map_err(|_| LinuxError::EFAULT)?;
            return Ok(0);
        }
        // Fallback: static input_id
        let id: [u8; 8] = [0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &id)
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(0);
    }

    // EVIOCGREP: _IOR('E', 0x03, unsigned int[2])
    if nr == 0x03 && dir == 2 {
        let (delay, period) = crate::drivers::input::evdev::with_device(device_idx, |dev| {
            (dev.rep_delay, dev.rep_period)
        })
        .unwrap_or((250, 33));
        let rep: [u8; 8] = [
            (delay & 0xFF) as u8,
            ((delay >> 8) & 0xFF) as u8,
            (delay >> 16) as u8,
            (delay >> 24) as u8,
            (period & 0xFF) as u8,
            ((period >> 8) & 0xFF) as u8,
            (period >> 16) as u8,
            (period >> 24) as u8,
        ];
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &rep)
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(0);
    }

    // EVIOCSREP: _IOW('E', 0x03, unsigned int[2])
    if nr == 0x03 && dir == 1 {
        let mut buf = [0u8; 8];
        crate::memory::user_space::UserSpaceMemory::copy_from_user(argp, &mut buf)
            .map_err(|_| LinuxError::EFAULT)?;
        let delay = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let period = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let _ = crate::drivers::input::evdev::with_device_mut(device_idx, |dev| {
            dev.rep_delay = delay;
            dev.rep_period = period;
        });
        return Ok(0);
    }

    // EVIOCGKEYCODE: _IOR('E', 0x04, unsigned int[2])
    if nr == 0x04 && dir == 2 {
        let pair: [u8; 8] = [0u8; 8];
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &pair)
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(0);
    }

    // EVIOCSKEYCODE: _IOW('E', 0x04, unsigned int[2])
    if nr == 0x04 && dir == 1 {
        return Ok(0);
    }

    // EVIOCGNAME(len): _IOC(_IOC_READ, 'E', 0x06, len)
    if nr == 0x06 && dir == 2 {
        let name = crate::drivers::input::evdev::with_device(device_idx, |dev| {
            let mut s = alloc::vec::Vec::new();
            s.extend_from_slice(dev.name.as_bytes());
            s.push(0); // null terminator
            s
        })
        .unwrap_or_else(|_| b"RustOS Input\0".to_vec());
        let len = core::cmp::min(size, name.len());
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &name[..len])
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(len as i32);
    }

    // EVIOCGPHYS(len): _IOC(_IOC_READ, 'E', 0x07, len)
    if nr == 0x07 && dir == 2 {
        let phys = crate::drivers::input::evdev::with_device(device_idx, |dev| {
            let mut s = alloc::vec::Vec::new();
            s.extend_from_slice(dev.phys.as_bytes());
            s.push(0);
            s
        })
        .unwrap_or_else(|_| b"isa0060/serio0/input0\0".to_vec());
        let len = core::cmp::min(size, phys.len());
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &phys[..len])
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(len as i32);
    }

    // EVIOCGUNIQ(len): _IOC(_IOC_READ, 'E', 0x08, len)
    if nr == 0x08 && dir == 2 {
        let uniq = crate::drivers::input::evdev::with_device(device_idx, |dev| {
            let mut s = alloc::vec::Vec::new();
            s.extend_from_slice(dev.uniq.as_bytes());
            s.push(0);
            s
        })
        .unwrap_or_else(|_| b"\0".to_vec());
        let len = core::cmp::min(size, uniq.len());
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &uniq[..len])
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(len as i32);
    }

    // EVIOCGPROP(len): _IOC(_IOC_READ, 'E', 0x09, len)
    if nr == 0x09 && dir == 2 {
        let props =
            crate::drivers::input::evdev::with_device(device_idx, |dev| dev.prop_bits.clone())
                .unwrap_or_else(|_| alloc::vec![0u8; 1]);
        let len = core::cmp::min(size, props.len());
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &props[..len])
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(len as i32);
    }

    // EVIOCGMTSLOTS(len): _IOC(_IOC_READ, 'E', 0x0a, len)
    if nr == 0x0a && dir == 2 {
        // Return zeros — no MT slots
        let len = core::cmp::min(size, 4);
        let buf = alloc::vec![0u8; len];
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &buf)
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(0);
    }

    // ── Variable-length, mask-size commands (Linux EVIOC_MASK_SIZE) ──

    // EVIOCGKEY(len): _IOC(_IOC_READ, 'E', 0x18, len)
    if nr == 0x18 && dir == 2 {
        let key_state =
            crate::drivers::input::evdev::with_device(device_idx, |dev| dev.key_state.clone())
                .unwrap_or_else(|_| alloc::vec![0u8; 1]);
        let len = core::cmp::min(size, key_state.len());
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &key_state[..len])
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(len as i32);
    }

    // EVIOCGLED(len): _IOC(_IOC_READ, 'E', 0x19, len)
    if nr == 0x19 && dir == 2 {
        let led_state =
            crate::drivers::input::evdev::with_device(device_idx, |dev| dev.led_state.clone())
                .unwrap_or_else(|_| alloc::vec![0u8; 1]);
        let len = core::cmp::min(size, led_state.len());
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &led_state[..len])
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(len as i32);
    }

    // EVIOCGSND(len): _IOC(_IOC_READ, 'E', 0x1a, len)
    if nr == 0x1a && dir == 2 {
        let snd_state =
            crate::drivers::input::evdev::with_device(device_idx, |dev| dev.snd_state.clone())
                .unwrap_or_else(|_| alloc::vec![0u8; 1]);
        let len = core::cmp::min(size, snd_state.len());
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &snd_state[..len])
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(len as i32);
    }

    // EVIOCGSW(len): _IOC(_IOC_READ, 'E', 0x1b, len)
    if nr == 0x1b && dir == 2 {
        let sw_state =
            crate::drivers::input::evdev::with_device(device_idx, |dev| dev.sw_state.clone())
                .unwrap_or_else(|_| alloc::vec![0u8; 1]);
        let len = core::cmp::min(size, sw_state.len());
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &sw_state[..len])
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(len as i32);
    }

    // EVIOCGBIT(ev, len): _IOC(_IOC_READ, 'E', 0x20 + ev, len)
    // Multi-number variable-length handler (Linux handle_eviocgbit)
    if nr >= 0x20 && nr <= 0x3F && dir == 2 {
        let ev_type = (nr - 0x20) as u16;
        let bits = crate::drivers::input::evdev::with_device(device_idx, |dev| {
            dev.get_capability_bitmap(ev_type).to_vec()
        })
        .unwrap_or_else(|_| {
            // Fallback to static bitmap if evdev device not found
            evdev_capability_bitmap_fallback(ev_type, size)
        });
        let len = core::cmp::min(size, bits.len());
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &bits[..len])
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(len as i32);
    }

    // EVIOCGABS(abs): _IOR('E', 0x40 + abs, struct input_absinfo) — 24 bytes
    if nr >= 0x40 && nr <= 0x7F && dir == 2 {
        let abs_code = (nr - 0x40) as u16;
        let abs_bytes = crate::drivers::input::evdev::with_device(device_idx, |dev| {
            if (abs_code as usize) < dev.absinfo.len() {
                dev.absinfo[abs_code as usize].to_bytes()
            } else {
                [0u8; 24]
            }
        })
        .unwrap_or([0u8; 24]);
        let len = core::cmp::min(size, 24);
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &abs_bytes[..len])
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(0);
    }

    // EVIOCSABS(abs): _IOW('E', 0xc0 + abs, struct input_absinfo) — 24 bytes
    if nr >= 0xc0 && nr <= 0xFF && dir == 1 {
        let abs_code = (nr - 0xc0) as u16;
        let mut buf = [0u8; 24];
        let len = core::cmp::min(size, 24);
        crate::memory::user_space::UserSpaceMemory::copy_from_user(argp, &mut buf[..len])
            .map_err(|_| LinuxError::EFAULT)?;
        let info = crate::drivers::input::evdev::InputAbsInfo::from_bytes(&buf);
        let _ = crate::drivers::input::evdev::with_device_mut(device_idx, |dev| {
            if (abs_code as usize) < dev.absinfo.len()
                && abs_code != crate::drivers::input::ABS_MT_SLOT
            {
                dev.absinfo[abs_code as usize] = info;
            }
        });
        return Ok(0);
    }

    // EVIOCGEFFECTS: _IOR('E', 0x84, int)
    if nr == 0x84 && dir == 2 {
        let n_effects: i32 = 0;
        crate::memory::user_space::UserSpaceMemory::copy_to_user(argp, &n_effects.to_ne_bytes())
            .map_err(|_| LinuxError::EFAULT)?;
        return Ok(0);
    }

    // EVIOCGRAB: _IOW('E', 0x90, int)
    if nr == 0x90 && dir == 1 {
        return Ok(0);
    }

    // EVIOCREVOKE: _IOW('E', 0x91, int)
    if nr == 0x91 && dir == 1 {
        return Ok(0);
    }

    // EVIOCGMASK: _IOR('E', 0x92, struct input_mask)
    if nr == 0x92 && dir == 2 {
        // Read the input_mask struct from userspace
        let mut mask_buf = [0u8; 16];
        crate::memory::user_space::UserSpaceMemory::copy_from_user(argp, &mut mask_buf)
            .map_err(|_| LinuxError::EFAULT)?;
        let mask = crate::drivers::input::evdev::InputMask::from_bytes(&mask_buf);
        // Return the capability bitmap for the requested type
        let bits = crate::drivers::input::evdev::with_device(device_idx, |dev| {
            dev.get_capability_bitmap(mask.mask_type as u16).to_vec()
        })
        .unwrap_or_else(|_| alloc::vec![0u8; 1]);
        let xfer_size = core::cmp::min(mask.codes_size as usize, bits.len());
        if mask.codes_ptr != 0 {
            crate::memory::user_space::UserSpaceMemory::copy_to_user(
                mask.codes_ptr,
                &bits[..xfer_size],
            )
            .map_err(|_| LinuxError::EFAULT)?;
        }
        return Ok(0);
    }

    // EVIOCSMASK: _IOW('E', 0x93, struct input_mask)
    if nr == 0x93 && dir == 1 {
        return Ok(0);
    }

    // EVIOCSCLOCKID: _IOW('E', 0xa0, int)
    if nr == 0xa0 && dir == 1 {
        let mut buf = [0u8; 4];
        crate::memory::user_space::UserSpaceMemory::copy_from_user(argp, &mut buf)
            .map_err(|_| LinuxError::EFAULT)?;
        let clkid = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        // Accept CLOCK_REALTIME(0), CLOCK_MONOTONIC(1), CLOCK_BOOTTIME(7)
        match clkid {
            0 | 1 | 7 => return Ok(0),
            _ => return Err(LinuxError::EINVAL),
        }
    }

    Err(LinuxError::ENOTTY)
}

/// Fallback capability bitmap when no evdev device is registered.
/// Used only as a last resort.
fn evdev_capability_bitmap_fallback(ev_type: u16, max_bytes: usize) -> alloc::vec::Vec<u8> {
    let len = max_bytes.max(1).min(128);
    let mut bits = alloc::vec![0u8; len];

    match ev_type {
        0 => {
            if len >= 1 {
                bits[0] |= 1 << 0;
            }
        }
        1 => {
            if len >= 8 {
                bits[0] = 0xFF;
                bits[1] = 0xFF;
                bits[2] = 0xFF;
                bits[3] = 0xFF;
                bits[4] = 0xFF;
                bits[5] = 0xFF;
                bits[6] = 0xFF;
                bits[7] |= 0x01;
            }
            if len >= 36 {
                bits[34] |= 0x01;
                bits[34] |= 0x02;
                bits[34] |= 0x04;
                bits[34] |= 0x08;
                bits[34] |= 0x10;
            }
        }
        2 => {
            if len >= 2 {
                bits[0] |= 0x03;
                bits[1] |= 0x01;
            }
        }
        3 => {
            if len >= 1 {
                bits[0] |= 0x03;
            }
        }
        _ => {}
    }

    bits
}

pub fn ioctl(fd: Fd, request: u64, argp: u64) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if let Ok(stat) = crate::vfs::vfs_fstat(fd) {
        if stat.rdev == ((10 << 8) | 135) {
            return handle_rtc_ioctl(request, argp);
        }
        if stat.rdev == ((10 << 8) | 130) {
            return handle_watchdog_ioctl(request, argp);
        }
    }

    if let Some(result) = crate::userfaultfd::ioctl(fd, request, argp) {
        return result;
    }

    match request {
        ioctl_req::SIOCADDRT | ioctl_req::SIOCDELRT | ioctl_req::SIOCRTMSG => {
            if argp == 0 && request != ioctl_req::SIOCRTMSG {
                return Err(LinuxError::EFAULT);
            }
            let stack = crate::net::network_stack();
            crate::net::routing::handle_route_ioctl(
                request,
                argp,
                stack.routing_table(),
                |iface| stack.get_interface(iface).is_some(),
                |iface, gw| stack.gateway_reachable_on_interface(iface, gw),
                |addr, buf| {
                    crate::memory::user_space::UserSpaceMemory::copy_from_user(addr, buf)
                        .map_err(|_| ())
                },
            )
            .map_err(|e| super::socket_ops::net_err_to_linux(e))
        }
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
            copy_value_to_user::<WinSize>(argp, &(winsize))?;
            Ok(0)
        }
        ioctl_req::TIOCSWINSZ => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            let winsize = copy_value_from_user::<WinSize>(argp)?;
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
            copy_value_to_user::<i32>(argp, &(pgrp))?;
            Ok(0)
        }
        ioctl_req::TIOCSPGRP => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            let pgrp = copy_value_from_user::<i32>(argp)?;
            tty_ops::tcsetpgrp(fd, pgrp)
        }
        ioctl_req::FIONREAD => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            let pending = tty_ops::tty_pending_read(fd).unwrap_or(0);
            copy_value_to_user::<i32>(argp, &(pending as i32))?;
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
        req if is_evdev_ioctl(req) => handle_evdev_ioctl(fd, req, argp),
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

fn handle_rtc_ioctl(request: u64, argp: u64) -> LinuxResult<i32> {
    if argp == 0 {
        return Err(LinuxError::EFAULT);
    }
    match request {
        // RTC_RD_TIME
        0x80247009 => {
            let time = crate::drivers::rtc::read_time().map_err(|_| LinuxError::EIO)?;
            copy_value_to_user::<crate::drivers::rtc::RtcTime>(argp, &(time))?;
            Ok(0)
        }
        // RTC_SET_TIME
        0x4024700a => {
            let time = copy_value_from_user::<crate::drivers::rtc::RtcTime>(argp)?;
            crate::drivers::rtc::write_time(&time).map_err(|_| LinuxError::EIO)?;
            Ok(0)
        }
        _ => Err(LinuxError::ENOTTY),
    }
}

fn handle_watchdog_ioctl(request: u64, argp: u64) -> LinuxResult<i32> {
    match request {
        // WDIOC_GETTIMEOUT
        0x80045706 => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            let timeout = crate::drivers::watchdog::get_timeout() as i32;
            copy_value_to_user::<i32>(argp, &(timeout))?;
            Ok(0)
        }
        // WDIOC_SETTIMEOUT
        0xc0045706 => {
            if argp == 0 {
                return Err(LinuxError::EFAULT);
            }
            let timeout = copy_value_from_user::<i32>(argp)?;
            if timeout <= 0 {
                return Err(LinuxError::EINVAL);
            }
            crate::drivers::watchdog::set_timeout(timeout as u32);
            copy_value_to_user::<i32>(argp, &timeout)?;
            Ok(0)
        }
        // WDIOC_KEEPALIVE
        0x80045705 => {
            crate::drivers::watchdog::kick();
            Ok(0)
        }
        _ => Err(LinuxError::ENOTTY),
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
