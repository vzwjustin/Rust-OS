//! PID fd — Process file descriptors
//!
//! Ported from Linux kernel/pid.c (pidfd_open, pidfd_send_signal) and
//! kernel/fork.c (pidfd_create).
//!
//! Provides:
//! - pidfd_open(): create a fd referencing a process
//! - pidfd_send_signal(): send signal to process via pidfd
//! - pidfd_getfd(): duplicate a fd from another process
//!
//! PID fds are closeable, pollable references to processes that
//! survive exec and can be passed between processes.

use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── PID fd flags ────────────────────────────────────────────────────────

pub const PIDFD_NONBLOCK: u32 = 0o4000; // O_NONBLOCK
pub const PIDFD_THREAD: u32 = 0o200; // O_EXCL
const PIDFD_SIGNAL_THREAD: u32 = 1 << 0;
const PIDFD_SIGNAL_THREAD_GROUP: u32 = 1 << 1;
const PIDFD_SIGNAL_PROCESS_GROUP: u32 = 1 << 2;
const PIDFD_SEND_SIGNAL_FLAGS: u32 =
    PIDFD_SIGNAL_THREAD | PIDFD_SIGNAL_THREAD_GROUP | PIDFD_SIGNAL_PROCESS_GROUP;

// ── Global state: pidfd → PID mapping ───────────────────────────────────

static PIDFDS: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());
static NEXT_PIDFD_ID: AtomicU32 = AtomicU32::new(1);

/// pidfd_open — create a file descriptor referring to a process.
///
/// Returns a non-negative fd on success, negative errno on failure.
pub fn pidfd_open(pid: i32, flags: u32) -> i32 {
    if flags & !(PIDFD_NONBLOCK | PIDFD_THREAD) != 0 {
        return -22; // EINVAL
    }

    if pid <= 0 {
        return -22; // EINVAL
    }

    // Verify the process exists
    let pm = crate::process::get_process_manager();
    if pm.get_process(pid as u32).is_none() {
        return -3; // ESRCH
    }

    let id = NEXT_PIDFD_ID.fetch_add(1, Ordering::SeqCst);
    PIDFDS.write().insert(id, pid as u32);

    // Register as a special fd
    let mut fd_flags: u32 = crate::vfs::OpenFlags::RDWR;
    if flags & PIDFD_NONBLOCK != 0 {
        fd_flags |= crate::vfs::OpenFlags::NONBLOCK;
    }

    let fd = crate::linux_compat::special_fd::register_pidfd(id, fd_flags);
    if fd < 0 {
        PIDFDS.write().remove(&id);
        return fd;
    }

    crate::serial_println!("[pidfd] open: pid={} fd={} id={}", pid, fd, id);
    fd
}

/// pidfd_send_signal — send a signal to the process referenced by a pidfd.
///
/// Returns 0 on success, negative errno on failure.
pub fn pidfd_send_signal(pidfd: i32, sig: i32, _info: u64, flags: u32) -> i32 {
    if sig < 0 || sig > 64 {
        return -22; // EINVAL
    }

    if flags & !PIDFD_SEND_SIGNAL_FLAGS != 0 {
        return -22; // EINVAL
    }

    if (flags & PIDFD_SEND_SIGNAL_FLAGS).count_ones() > 1 {
        return -22; // EINVAL
    }

    if flags != 0 {
        return -95; // ENOTSUP
    }

    // Look up the pidfd
    let id = match crate::linux_compat::special_fd::get_pidfd_id(pidfd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    let target_pid = match PIDFDS.read().get(&id) {
        Some(&pid) => pid,
        None => return -9, // EBADF
    };

    // Verify the process still exists
    let pm = crate::process::get_process_manager();
    if pm.get_process(target_pid).is_none() {
        return -3; // ESRCH
    }

    // Deliver the signal using the signal_ops layer
    if sig == 0 {
        // Signal 0: just check existence (already done above)
        return 0;
    }

    // Add to pending signals
    pm.with_process_mut(target_pid, |pcb| {
        pcb.pending_signals.push(sig as u32);
    });

    // Wake if blocked
    if let Some(pcb) = pm.get_process(target_pid) {
        if matches!(
            pcb.state,
            crate::process::ProcessState::Sleeping | crate::process::ProcessState::Blocked
        ) {
            let _ = pm.unblock_process(target_pid);
        }
    }

    crate::serial_println!(
        "[pidfd] send_signal: pidfd={} pid={} sig={}",
        pidfd,
        target_pid,
        sig
    );
    0
}

/// pidfd_getfd — duplicate a file descriptor from another process.
///
/// Returns a new fd in the caller's fd table that refers to the same
/// file description as the target process's fd.
pub fn pidfd_getfd(pidfd: i32, target_fd: i32, flags: u32) -> i32 {
    if target_fd < 0 {
        return -22; // EINVAL
    }
    if flags != 0 {
        return -22; // EINVAL
    }

    // Look up the pidfd
    let id = match crate::linux_compat::special_fd::get_pidfd_id(pidfd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    let target_pid = match PIDFDS.read().get(&id) {
        Some(&pid) => pid,
        None => return -9,
    };

    // For security, only root can steal fds from other processes
    let current_pid = crate::process::current_pid();
    if current_pid != target_pid {
        let pm = crate::process::get_process_manager();
        if let Some(pcb) = pm.get_process(current_pid) {
            if pcb.euid != 0 {
                return -1; // EPERM
            }
        }
    }

    // Duplicate the fd — use VFS dup
    match crate::vfs::get_vfs().dup(target_fd) {
        Ok(new_fd) => new_fd,
        Err(_) => -9, // EBADF
    }
}

/// Get the PID associated with a pidfd. Returns None if not a pidfd.
pub fn get_pid(pidfd: i32) -> Option<u32> {
    let id = crate::linux_compat::special_fd::get_pidfd_id(pidfd)?;
    PIDFDS.read().get(&id).copied()
}

/// Get PID associated with an internal pidfd id.
pub fn get_pid_by_id(id: u32) -> Option<u32> {
    PIDFDS.read().get(&id).copied()
}

/// Close a pidfd (called when the fd is closed).
pub fn close_pidfd(id: u32) {
    PIDFDS.write().remove(&id);
}

/// Initialize the pidfd subsystem.
pub fn init() {
    crate::serial_println!("[pidfd] pidfd subsystem initialized");
}
