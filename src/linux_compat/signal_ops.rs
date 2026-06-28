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
use crate::process::{self, ProcessState};

lazy_static! {
    static ref SIGNAL_MASKS: Mutex<BTreeMap<u32, AtomicU64>> = Mutex::new(BTreeMap::new());
    static ref SIGNAL_STACKS: Mutex<BTreeMap<u32, SignalStack>> = Mutex::new(BTreeMap::new());
}

/// Per-process alternate signal stack
#[derive(Clone, Copy, Debug, Default)]
struct SignalStack {
    sp: u64,
    flags: i32,
    size: u64,
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

fn pending_signal_set(pid: u32) -> SigSet {
    let process_manager = process::get_process_manager();
    let mut pending_set: SigSet = 0;

    if let Some(pcb) = process_manager.get_process(pid) {
        for sig in &pcb.pending_signals {
            if *sig >= 1 && *sig <= 64 {
                pending_set |= 1u64 << (sig - 1);
            }
        }
    }

    pending_set
}

fn first_deliverable_signal(set: SigSet, blocked: SigSet, pending: SigSet) -> Option<i32> {
    let candidates = set & pending & !blocked;
    if candidates == 0 {
        return None;
    }
    for sig in 1..=64i32 {
        if candidates & (1u64 << (sig - 1)) != 0 {
            return Some(sig);
        }
    }
    None
}

fn consume_pending_signal(pid: u32, signum: i32) {
    let process_manager = process::get_process_manager();
    process_manager.with_process_mut(pid, |pcb| {
        pcb.pending_signals.retain(|s| *s != signum as u32);
    });
}

fn wake_if_blocked(pid: u32) {
    let process_manager = process::get_process_manager();
    if let Some(pcb) = process_manager.get_process(pid) {
        if matches!(pcb.state, ProcessState::Sleeping | ProcessState::Blocked) {
            let _ = process_manager.unblock_process(pid);
        }
    }
}

fn deliver_signal_to_pid(target_pid: u32, sig: i32) -> LinuxResult<()> {
    if sig == 0 {
        let process_manager = process::get_process_manager();
        if process_manager.get_process(target_pid).is_none() {
            return Err(LinuxError::ESRCH);
        }
        return Ok(());
    }

    if sig < 1 || sig > 64 {
        return Err(LinuxError::EINVAL);
    }

    let process_manager = process::get_process_manager();
    if process_manager.get_process(target_pid).is_none() {
        return Err(LinuxError::ESRCH);
    }

    process_manager.with_process_mut(target_pid, |pcb| {
        pcb.pending_signals.push(sig as u32);
    });
    wake_if_blocked(target_pid);
    Ok(())
}

fn deliver_signal_to_pgid(pgid: u32, sig: i32) -> LinuxResult<()> {
    let process_manager = process::get_process_manager();
    let mut delivered = false;

    for (pid, _, _, _) in process_manager.list_processes() {
        if let Some(pcb) = process_manager.get_process(pid) {
            if pcb.pgid == pgid {
                deliver_signal_to_pid(pid, sig)?;
                delivered = true;
            }
        }
    }

    if sig == 0 && !delivered {
        return Err(LinuxError::ESRCH);
    }

    Ok(())
}

fn broadcast_signal(sig: i32) -> LinuxResult<()> {
    let process_manager = process::get_process_manager();
    let mut delivered = false;

    for (pid, _, _, _) in process_manager.list_processes() {
        if process_manager.get_process(pid).is_some() {
            let _ = deliver_signal_to_pid(pid, sig);
            delivered = true;
        }
    }

    if sig == 0 && !delivered {
        return Err(LinuxError::ESRCH);
    }

    Ok(())
}

fn wait_for_signal(set: SigSet, blocked: SigSet, timeout: Option<u64>) -> LinuxResult<i32> {
    let pid = process::current_pid();
    let start = crate::time::system_time();

    loop {
        let pending = pending_signal_set(pid);
        if let Some(sig) = first_deliverable_signal(set, blocked, pending) {
            consume_pending_signal(pid, sig);
            return Ok(sig);
        }

        if let Some(deadline) = timeout {
            if crate::time::system_time() >= deadline {
                return Err(LinuxError::EAGAIN);
            }
        }

        crate::time::sleep_us(1_000);

        if timeout.is_none() {
            let _ = start;
        }
    }
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

    let pid = process::current_pid();
    unsafe {
        *set = pending_signal_set(pid);
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

    let pid = process::current_pid();
    let new_mask = unsafe { *mask };
    let old_mask = signal_mask_for(pid);
    update_signal_mask(pid, |_| new_mask);

    loop {
        let pending = pending_signal_set(pid);
        let deliverable = pending & !new_mask;
        if deliverable != 0 {
            update_signal_mask(pid, |_| old_mask);
            return Err(LinuxError::EINTR);
        }
        crate::time::sleep_us(1_000);
    }
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

    const SS_DISABLE: i32 = 2;
    const SS_ONSTACK: i32 = 1;

    let pid = process::current_pid();

    if !old_ss.is_null() {
        let stacks = SIGNAL_STACKS.lock();
        let stack = stacks.get(&pid).copied().unwrap_or_default();
        unsafe {
            let out = old_ss as *mut u64;
            *out = stack.sp;
            *(old_ss.add(8) as *mut i32) = if stack.flags & SS_DISABLE != 0 {
                SS_DISABLE
            } else if stack.size > 0 {
                0
            } else {
                SS_DISABLE
            };
            *(old_ss.add(16) as *mut u64) = stack.size;
        }
    }

    if !ss.is_null() {
        unsafe {
            let input_sp = *(ss as *const u64);
            let input_flags = *(ss.add(8) as *const i32);
            let input_size = *(ss.add(16) as *const u64);

            if input_flags & SS_DISABLE != 0 {
                let mut stacks = SIGNAL_STACKS.lock();
                stacks.insert(
                    pid,
                    SignalStack {
                        sp: 0,
                        flags: SS_DISABLE,
                        size: 0,
                    },
                );
            } else {
                if input_sp == 0 || input_size < 2048 {
                    return Err(LinuxError::ENOMEM);
                }
                let mut stacks = SIGNAL_STACKS.lock();
                stacks.insert(
                    pid,
                    SignalStack {
                        sp: input_sp,
                        flags: input_flags & !SS_ONSTACK,
                        size: input_size,
                    },
                );
            }
        }
    }

    Ok(0)
}

/// sigtimedwait - wait for queued signals
pub fn sigtimedwait(
    set: *const SigSet,
    _info: *mut u8,
    timeout: *const TimeSpec,
) -> LinuxResult<i32> {
    inc_ops();

    if set.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let wait_set = unsafe { *set };
    let blocked = signal_mask_for(process::current_pid());

    let deadline = if timeout.is_null() {
        None
    } else {
        unsafe {
            let ts = &*timeout;
            Some(crate::time::system_time() + ts.tv_sec as u64 + ts.tv_nsec as u64 / 1_000_000_000)
        }
    };

    wait_for_signal(wait_set, blocked, deadline)
}

/// sigwaitinfo - wait for queued signals
pub fn sigwaitinfo(set: *const SigSet, info: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    sigtimedwait(set, info, core::ptr::null())
}

/// sigqueue - queue a signal and data to a process
pub fn sigqueue(pid: Pid, sig: i32, _value: i32) -> LinuxResult<i32> {
    inc_ops();

    if sig < 0 || sig > 64 {
        return Err(LinuxError::EINVAL);
    }

    if pid <= 0 {
        return Err(LinuxError::EINVAL);
    }

    deliver_signal_to_pid(pid as u32, sig)?;
    Ok(0)
}

/// pause - wait for signal
pub fn pause() -> LinuxResult<i32> {
    inc_ops();

    let pid = process::current_pid();
    let blocked = signal_mask_for(pid);

    loop {
        let pending = pending_signal_set(pid);
        if first_deliverable_signal(!0, blocked, pending).is_some() {
            return Err(LinuxError::EINTR);
        }
        crate::time::sleep_us(1_000);
    }
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
        *set = !0;
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

    if sig < 0 || sig > 64 {
        return Err(LinuxError::EINVAL);
    }

    let process_manager = process::get_process_manager();

    if pid > 0 {
        deliver_signal_to_pid(pid as u32, sig)?;
    } else if pid == 0 {
        let current = process::current_pid();
        let pgid = process_manager
            .get_process(current)
            .map(|pcb| pcb.pgid)
            .ok_or(LinuxError::ESRCH)?;
        deliver_signal_to_pgid(pgid, sig)?;
    } else if pid == -1 {
        broadcast_signal(sig)?;
    } else {
        let pgid = (-pid) as u32;
        deliver_signal_to_pgid(pgid, sig)?;
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

        assert!(sigaction(signal::SIGKILL, &act, core::ptr::null_mut()).is_err());
        assert!(sigaction(signal::SIGINT, &act, core::ptr::null_mut()).is_ok());
    }
}
