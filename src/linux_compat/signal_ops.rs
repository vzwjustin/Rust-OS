//! Linux signal handling APIs
//!
//! This module implements Linux-compatible signal operations including
//! sigaction, sigprocmask, sigpending, and real-time signal support.

extern crate alloc;

use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU64, Ordering};

use lazy_static::lazy_static;
use spin::Mutex;

use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::process;

lazy_static! {
    static ref SIGNAL_MASKS: Mutex<BTreeMap<u32, AtomicU64>> = Mutex::new(BTreeMap::new());
}

fn signal_mask_for(pid: u32) -> u64 {
    let masks = SIGNAL_MASKS.lock();
    masks
        .get(&pid)
        .map(|m| m.load(Ordering::SeqCst))
        .unwrap_or(0)
}

fn update_signal_mask<F>(pid: u32, f: F)
where
    F: FnOnce(u64) -> u64,
{
    let mut masks = SIGNAL_MASKS.lock();
    let entry = masks.entry(pid).or_insert_with(|| AtomicU64::new(0));
    let current = entry.load(Ordering::SeqCst);
    entry.store(f(current), Ordering::SeqCst);
}

/// Operation counter for statistics
static SIGNAL_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize signal operations subsystem
pub fn init_signal_operations() {
    SIGNAL_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of signal operations performed
pub fn get_operation_count() -> u64 {
    SIGNAL_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    SIGNAL_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Signal action constants
pub mod sig_action {
    /// Default action
    pub const SIG_DFL: usize = 0;
    /// Ignore signal
    pub const SIG_IGN: usize = 1;
}

/// Signal mask operation constants
pub mod sig_how {
    /// Block signals
    pub const SIG_BLOCK: i32 = 0;
    /// Unblock signals
    pub const SIG_UNBLOCK: i32 = 1;
    /// Set signal mask
    pub const SIG_SETMASK: i32 = 2;
}

/// sigaction - examine and change signal action
pub fn sigaction(signum: i32, act: *const SigAction, oldact: *mut SigAction) -> LinuxResult<i32> {
    inc_ops();

    // Validate signal number
    if signum < 1 || signum > 64 {
        return Err(LinuxError::EINVAL);
    }

    // SIGKILL and SIGSTOP cannot be caught or ignored
    if signum == signal::SIGKILL || signum == signal::SIGSTOP {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Save old action if requested
    if !oldact.is_null() {
        unsafe {
            (*oldact).sa_handler = sig_action::SIG_DFL;
            (*oldact).sa_flags = 0;
            (*oldact).sa_restorer = 0;
            (*oldact).sa_mask = 0;
        }
    }

    // TODO: Set new action if provided
    if !act.is_null() {
        // Validate and install new signal handler
    }

    Ok(0)
}

/// rt_sigaction - real-time signal action (similar to sigaction)
pub fn rt_sigaction(
    signum: i32,
    act: *const SigAction,
    oldact: *mut SigAction,
    sigsetsize: usize,
) -> LinuxResult<i32> {
    inc_ops();

    if sigsetsize != 8 {
        return Err(LinuxError::EINVAL);
    }

    if signum < 1 || signum > 64 {
        return Err(LinuxError::EINVAL);
    }

    if signum == signal::SIGKILL || signum == signal::SIGSTOP {
        return Err(LinuxError::EINVAL);
    }

    let pid = process::current_pid();
    let process_manager = process::get_process_manager();

    if !oldact.is_null() {
        let old_handler = process_manager
            .get_process(pid)
            .and_then(|pcb| pcb.signal_handlers.get(&(signum as u32)).copied())
            .unwrap_or(sig_action::SIG_DFL as u64);
        unsafe {
            (*oldact).sa_handler = old_handler as usize;
            (*oldact).sa_flags = 0;
            (*oldact).sa_restorer = 0;
            (*oldact).sa_mask = signal_mask_for(pid);
        }
    }

    if !act.is_null() {
        unsafe {
            let handler = (*act).sa_handler as u64;
            process_manager.with_process_mut(pid, |pcb| {
                pcb.signal_handlers.insert(signum as u32, handler);
            });
        }
    }

    Ok(0)
}

/// sigprocmask - examine and change blocked signals
pub fn sigprocmask(how: i32, set: *const SigSet, oldset: *mut SigSet) -> LinuxResult<i32> {
    inc_ops();

    let pid = process::current_pid();

    if !oldset.is_null() {
        unsafe {
            *oldset = signal_mask_for(pid);
        }
    }

    if !set.is_null() {
        unsafe {
            let mask = *set;
            match how {
                sig_how::SIG_BLOCK => {
                    update_signal_mask(pid, |current| current | mask);
                }
                sig_how::SIG_UNBLOCK => {
                    update_signal_mask(pid, |current| current & !mask);
                }
                sig_how::SIG_SETMASK => {
                    update_signal_mask(pid, |_| mask);
                }
                _ => return Err(LinuxError::EINVAL),
            }
        }
    }

    Ok(0)
}

/// rt_sigprocmask - real-time signal mask
pub fn rt_sigprocmask(
    how: i32,
    set: *const SigSet,
    oldset: *mut SigSet,
    sigsetsize: usize,
) -> LinuxResult<i32> {
    inc_ops();

    if sigsetsize != 8 {
        return Err(LinuxError::EINVAL);
    }

    sigprocmask(how, set, oldset)
}

/// sigpending - examine pending signals
pub fn sigpending(set: *mut SigSet) -> LinuxResult<i32> {
    inc_ops();

    if set.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Get pending signals
    unsafe {
        *set = 0; // No pending signals for now
    }

    Ok(0)
}

/// rt_sigpending - real-time pending signals
pub fn rt_sigpending(set: *mut SigSet, sigsetsize: usize) -> LinuxResult<i32> {
    inc_ops();

    if sigsetsize != 8 {
        return Err(LinuxError::EINVAL);
    }

    sigpending(set)
}

/// sigsuspend - wait for signal
pub fn sigsuspend(mask: *const SigSet) -> LinuxResult<i32> {
    inc_ops();

    if mask.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Suspend process until signal arrives
    // This should never return normally, only via signal handler
    // For now, just return EINTR as if interrupted
    Err(LinuxError::EINTR)
}

/// rt_sigsuspend - real-time signal suspend
pub fn rt_sigsuspend(mask: *const SigSet, sigsetsize: usize) -> LinuxResult<i32> {
    inc_ops();

    if sigsetsize != 8 {
        return Err(LinuxError::EINVAL);
    }

    sigsuspend(mask)
}

/// sigaltstack - set/get signal stack context
pub fn sigaltstack(ss: *const u8, old_ss: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    // TODO: Set alternate signal stack
    // For now, just copy if old_ss is provided
    if !old_ss.is_null() {
        unsafe {
            core::ptr::write_bytes(old_ss, 0, 24); // stack_t is 24 bytes
        }
    }

    Ok(0)
}

/// sigtimedwait - wait for queued signals
pub fn sigtimedwait(
    set: *const SigSet,
    info: *mut u8, // siginfo_t
    timeout: *const TimeSpec,
) -> LinuxResult<i32> {
    inc_ops();

    if set.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Wait for signal with timeout
    // Return signal number if caught, or error
    Err(LinuxError::EAGAIN)
}

/// sigwaitinfo - wait for queued signals
pub fn sigwaitinfo(set: *const SigSet, info: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    sigtimedwait(set, info, core::ptr::null())
}

/// sigqueue - queue a signal and data to a process
pub fn sigqueue(pid: Pid, sig: i32, value: i32) -> LinuxResult<i32> {
    inc_ops();

    if sig < 0 || sig > 64 {
        return Err(LinuxError::EINVAL);
    }

    if pid <= 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Queue signal to process
    Ok(0)
}

/// pause - wait for signal
pub fn pause() -> LinuxResult<i32> {
    inc_ops();

    // TODO: Suspend until signal arrives
    // Always returns EINTR when interrupted by signal
    Err(LinuxError::EINTR)
}

/// Signal set manipulation helpers

/// sigemptyset - initialize empty signal set
pub fn sigemptyset(set: *mut SigSet) -> LinuxResult<i32> {
    if set.is_null() {
        return Err(LinuxError::EFAULT);
    }

    unsafe {
        *set = 0;
    }

    Ok(0)
}

/// sigfillset - initialize full signal set
pub fn sigfillset(set: *mut SigSet) -> LinuxResult<i32> {
    if set.is_null() {
        return Err(LinuxError::EFAULT);
    }

    unsafe {
        *set = !0; // All bits set
    }

    Ok(0)
}

/// sigaddset - add signal to set
pub fn sigaddset(set: *mut SigSet, signum: i32) -> LinuxResult<i32> {
    if set.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if signum < 1 || signum > 64 {
        return Err(LinuxError::EINVAL);
    }

    unsafe {
        *set |= 1u64 << (signum - 1);
    }

    Ok(0)
}

/// sigdelset - remove signal from set
pub fn sigdelset(set: *mut SigSet, signum: i32) -> LinuxResult<i32> {
    if set.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if signum < 1 || signum > 64 {
        return Err(LinuxError::EINVAL);
    }

    unsafe {
        *set &= !(1u64 << (signum - 1));
    }

    Ok(0)
}

/// sigismember - test if signal is in set
pub fn sigismember(set: *const SigSet, signum: i32) -> LinuxResult<i32> {
    if set.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if signum < 1 || signum > 64 {
        return Err(LinuxError::EINVAL);
    }

    unsafe {
        let is_member = (*set & (1u64 << (signum - 1))) != 0;
        Ok(if is_member { 1 } else { 0 })
    }
}

/// kill - send signal to a process
pub fn kill(pid: Pid, sig: i32) -> LinuxResult<i32> {
    inc_ops();

    // Validate signal number (0 is valid - means just check permissions)
    if sig < 0 || sig > 64 {
        return Err(LinuxError::EINVAL);
    }

    // pid > 0: send to process `pid`
    // pid == 0: send to all processes in same process group
    // pid == -1: send to all processes we can send to
    // pid < -1: send to process group -pid

    if pid > 0 {
        // Verify process exists
        let process_manager = process::get_process_manager();
        if process_manager.get_process(pid as u32).is_none() {
            return Err(LinuxError::ESRCH);
        }

        // TODO: Actually deliver the signal
        // For now, just succeed
    } else if pid == 0 {
        // TODO: Send to process group
    } else if pid == -1 {
        // TODO: Send to all processes
    } else {
        // TODO: Send to process group -pid
    }

    Ok(0)
}

#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_sigset_operations() {
        let mut set: SigSet = 0;

        assert!(sigemptyset(&mut set).is_ok());
        assert_eq!(set, 0);

        assert!(sigaddset(&mut set, signal::SIGINT).is_ok());
        assert_eq!(sigismember(&set, signal::SIGINT).unwrap(), 1);

        assert!(sigdelset(&mut set, signal::SIGINT).is_ok());
        assert_eq!(sigismember(&set, signal::SIGINT).unwrap(), 0);

        assert!(sigfillset(&mut set).is_ok());
        assert_eq!(set, !0);
    }

    #[test_case]
    fn test_signal_validation() {
        let act = SigAction {
            sa_handler: sig_action::SIG_DFL,
            sa_flags: 0,
            sa_restorer: 0,
            sa_mask: 0,
        };

        // SIGKILL cannot be caught
        assert!(sigaction(signal::SIGKILL, &act, core::ptr::null_mut()).is_err());

        // Valid signal
        assert!(sigaction(signal::SIGINT, &act, core::ptr::null_mut()).is_ok());
    }
}
