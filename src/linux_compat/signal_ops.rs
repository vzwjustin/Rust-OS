//! Linux signal handling APIs
//!
//! This module implements Linux-compatible signal operations including
//! sigaction, sigprocmask, sigpending, and real-time signal support.

use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxError, LinuxResult};

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

    sigaction(signum, act, oldact)
}

/// sigprocmask - examine and change blocked signals
pub fn sigprocmask(how: i32, set: *const SigSet, oldset: *mut SigSet) -> LinuxResult<i32> {
    inc_ops();

    // TODO: Save old mask if requested
    if !oldset.is_null() {
        unsafe {
            *oldset = 0; // No signals blocked by default
        }
    }

    // Update signal mask if new set provided
    if !set.is_null() {
        match how {
            sig_how::SIG_BLOCK => {
                // TODO: Add signals from set to blocked mask
            }
            sig_how::SIG_UNBLOCK => {
                // TODO: Remove signals from set from blocked mask
            }
            sig_how::SIG_SETMASK => {
                // TODO: Set blocked mask to set
            }
            _ => return Err(LinuxError::EINVAL),
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
