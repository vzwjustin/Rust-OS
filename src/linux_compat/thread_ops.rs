//! Threading and synchronization operations
//!
//! This module implements Linux threading operations including
//! futex, clone, thread-local storage, and pthread-compatible functions.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use spin::RwLock;

use super::process_ops;
use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::memory::user_space::UserSpaceMemory;
use crate::process;

/// FS base MSR (x86_64)
const MSR_FS_BASE: u32 = crate::arch::x86::msr::MSR_FS_BASE;
/// GS base MSR (x86_64)
const MSR_GS_BASE: u32 = crate::arch::x86::msr::MSR_GS_BASE;

static CLEAR_CHILD_TID: AtomicU64 = AtomicU64::new(0);
static MEMBARRIER_REGISTRATIONS: AtomicI32 = AtomicI32::new(0);

/// Return the current clear-child-TID userspace address.
pub fn clear_child_tid_address() -> u64 {
    CLEAR_CHILD_TID.load(Ordering::SeqCst)
}

/// Per-thread FS/GS base addresses keyed by tid.
static TLS_FS_BASE: RwLock<BTreeMap<Pid, u64>> = RwLock::new(BTreeMap::new());
static TLS_GS_BASE: RwLock<BTreeMap<Pid, u64>> = RwLock::new(BTreeMap::new());

/// Per-thread robust futex list heads keyed by tid (stored as address).
static ROBUST_LISTS: RwLock<BTreeMap<Pid, usize>> = RwLock::new(BTreeMap::new());

/// Legacy x86 thread area entries keyed by tid.
static THREAD_AREAS: RwLock<BTreeMap<Pid, [u8; 32]>> = RwLock::new(BTreeMap::new());

fn copy_pid_to_user(dst: *mut Pid, value: Pid) -> LinuxResult<()> {
    UserSpaceMemory::copy_to_user(dst as u64, &value.to_ne_bytes()).map_err(|_| LinuxError::EFAULT)
}

fn copy_u64_to_user_addr(dst: u64, value: u64) -> LinuxResult<()> {
    UserSpaceMemory::copy_to_user(dst, &value.to_ne_bytes()).map_err(|_| LinuxError::EFAULT)
}

fn copy_u64_from_user_addr(src: u64) -> LinuxResult<u64> {
    let mut bytes = [0u8; core::mem::size_of::<u64>()];
    UserSpaceMemory::copy_from_user(src, &mut bytes).map_err(|_| LinuxError::EFAULT)?;
    Ok(u64::from_ne_bytes(bytes))
}

fn copy_usize_to_user(dst: *mut usize, value: usize) -> LinuxResult<()> {
    UserSpaceMemory::copy_to_user(dst as u64, &value.to_ne_bytes()).map_err(|_| LinuxError::EFAULT)
}

/// A thread blocked on a futex, together with the bitset it is waiting on.
/// Non-bitset operations use `FUTEX_BITSET_MATCH_ANY`, so FUTEX_WAKE wakes them
/// the same as a bitset wake with an all-ones mask.
#[derive(Clone, Copy)]
struct FutexWaiter {
    pid: Pid,
    bitset: u32,
    /// If non-zero, this waiter is a FUTEX_WAIT_REQUEUE_PI caller waiting
    /// on `uaddr` to be requeued to the PI futex at this address.
    /// FUTEX_CMP_REQUEUE_PI checks this field to find eligible waiters.
    pi_requeue_target: usize,
}

/// Futex wait queues keyed by userspace address.
static FUTEX_WAITERS: RwLock<BTreeMap<usize, Vec<FutexWaiter>>> = RwLock::new(BTreeMap::new());

#[inline(always)]
unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nostack, preserves_flags)
    );
}

fn current_tid() -> Pid {
    process::current_pid() as Pid
}

fn send_signal_to_thread(tid: Pid, sig: i32) -> LinuxResult<i32> {
    if sig != 0 && (sig < 0 || sig > 64) {
        return Err(LinuxError::EINVAL);
    }

    let target = tid as u32;
    process::get_process_manager()
        .with_process_mut(target, |pcb| {
            if sig != 0 {
                pcb.pending_signals.push(sig as u32);
            }
        })
        .ok_or(LinuxError::ESRCH)?;

    // Wake the thread if it's blocked (e.g., in a blocking syscall)
    if sig != 0 {
        let pm = process::get_process_manager();
        let pid = tid.try_into().map_err(|_| LinuxError::EINVAL)?;
        let _ = pm.unblock_process(pid);
    }

    Ok(0)
}

/// Operation counter for statistics
static THREAD_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize thread operations subsystem
pub fn init_thread_operations() {
    THREAD_OPS_COUNT.store(0, Ordering::Relaxed);
    TLS_FS_BASE.write().clear();
    TLS_GS_BASE.write().clear();
    ROBUST_LISTS.write().clear();
    THREAD_AREAS.write().clear();
}

/// Get number of thread operations performed
pub fn get_operation_count() -> u64 {
    THREAD_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    THREAD_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

// ============================================================================
// Clone Flags
// ============================================================================

pub mod clone_flags {
    /// Set if VM shared between processes
    pub const CLONE_VM: u64 = 0x00000100;
    /// Set if fs info shared between processes
    pub const CLONE_FS: u64 = 0x00000200;
    /// Set if open files shared between processes
    pub const CLONE_FILES: u64 = 0x00000400;
    /// Set if signal handlers shared
    pub const CLONE_SIGHAND: u64 = 0x00000800;
    /// Set if we want to have the same parent as the cloner
    pub const CLONE_PARENT: u64 = 0x00008000;
    /// Set if we want to let tracing continue on the child
    pub const CLONE_PTRACE: u64 = 0x00002000;
    /// Set if the parent wants the child to wake it up on mm_release
    pub const CLONE_VFORK: u64 = 0x00004000;
    /// Set to add to the same thread group
    pub const CLONE_THREAD: u64 = 0x00010000;
    /// New mount namespace
    pub const CLONE_NEWNS: u64 = 0x00020000;
    /// Share system V SEM_UNDO semantics
    pub const CLONE_SYSVSEM: u64 = 0x00040000;
    /// Create a thread-local storage for the child
    pub const CLONE_SETTLS: u64 = 0x00080000;
    /// Set the TID in the parent
    pub const CLONE_PARENT_SETTID: u64 = 0x00100000;
    /// Clear the TID in the child
    pub const CLONE_CHILD_CLEARTID: u64 = 0x00200000;
    /// Set the TID in the child
    pub const CLONE_CHILD_SETTID: u64 = 0x01000000;
    /// New cgroup namespace
    pub const CLONE_NEWCGROUP: u64 = 0x02000000;
    /// New UTS namespace
    pub const CLONE_NEWUTS: u64 = 0x04000000;
    /// New IPC namespace
    pub const CLONE_NEWIPC: u64 = 0x08000000;
    /// New user namespace
    pub const CLONE_NEWUSER: u64 = 0x10000000;
    /// New PID namespace
    pub const CLONE_NEWPID: u64 = 0x20000000;
    /// New network namespace
    pub const CLONE_NEWNET: u64 = 0x40000000;
    /// Clone I/O context
    pub const CLONE_IO: u64 = 0x80000000;
}

// ============================================================================
// Futex Operations
// ============================================================================

pub mod futex_op {
    /// Wait on futex
    pub const FUTEX_WAIT: i32 = 0;
    /// Wake waiters on futex
    pub const FUTEX_WAKE: i32 = 1;
    /// Requeue waiters
    pub const FUTEX_REQUEUE: i32 = 3;
    /// Compare and requeue
    pub const FUTEX_CMP_REQUEUE: i32 = 4;
    /// Wait with timeout
    pub const FUTEX_WAIT_BITSET: i32 = 9;
    /// Wake with bitset
    pub const FUTEX_WAKE_BITSET: i32 = 10;
    /// Lock PI futex
    pub const FUTEX_LOCK_PI: i32 = 6;
    /// Unlock PI futex
    pub const FUTEX_UNLOCK_PI: i32 = 7;
    /// Try lock PI futex
    pub const FUTEX_TRYLOCK_PI: i32 = 8;
    /// Wait on PI futex
    pub const FUTEX_WAIT_REQUEUE_PI: i32 = 11;
    /// Requeue to PI futex
    pub const FUTEX_CMP_REQUEUE_PI: i32 = 12;

    /// Private futex flag
    pub const FUTEX_PRIVATE_FLAG: i32 = 128;
    /// Clock realtime flag
    pub const FUTEX_CLOCK_REALTIME: i32 = 256;

    /// Bitset matching any waiter (used by non-bitset waits/wakes).
    pub const FUTEX_BITSET_MATCH_ANY: u32 = 0xffff_ffff;
}

// ============================================================================
// Clone and Thread Creation
// ============================================================================

/// clone - create a child process or thread
pub fn clone(
    flags: u64,
    stack: *mut u8,
    parent_tid: *mut Pid,
    child_tid: *mut Pid,
    tls: u64,
) -> LinuxResult<Pid> {
    inc_ops();

    let parent_pid = process::current_pid();
    let child_pid = process::integration::get_integration_manager()
        .fork_process(parent_pid)
        .map_err(|_| LinuxError::EAGAIN)?;
    let linux_child_pid: Pid = child_pid.try_into().map_err(|_| LinuxError::EAGAIN)?;

    // Apply clone-specific flags on the child PCB.
    let pm = process::get_process_manager();

    // CLONE_SETTLS: set the child's FS_BASE for thread-local storage.
    if flags & clone_flags::CLONE_SETTLS != 0 {
        let _ = pm.set_fs_base(child_pid, tls);
    }

    // CLONE_PARENT_SETTID: write child's TID into parent's tidptr.
    if flags & clone_flags::CLONE_PARENT_SETTID != 0 && !parent_tid.is_null() {
        copy_pid_to_user(parent_tid, linux_child_pid)?;
    }

    // CLONE_CHILD_CLEARTID: store the tidptr so do_exit can futex-wake it.
    if flags & clone_flags::CLONE_CHILD_CLEARTID != 0 && !child_tid.is_null() {
        let _ = pm.set_clear_child_tid(child_pid, child_tid as u64);
    }

    // CLONE_CHILD_SETTID: write child's TID into child's tidptr.
    if flags & clone_flags::CLONE_CHILD_SETTID != 0 && !child_tid.is_null() {
        copy_pid_to_user(child_tid, linux_child_pid)?;
    }

    // Override the child's stack pointer if a new stack was provided.
    if stack as u64 != 0 {
        let _ = pm.set_child_stack(child_pid, stack as u64);
    }

    Ok(linux_child_pid)
}

/// set_tid_address - set pointer to thread ID
pub fn set_tid_address(tidptr: *mut Pid) -> Pid {
    inc_ops();

    CLEAR_CHILD_TID.store(tidptr as u64, Ordering::SeqCst);
    current_tid()
}

/// gettid - get thread ID
pub fn gettid() -> Pid {
    inc_ops();
    current_tid()
}

/// tkill - send signal to thread
pub fn tkill(tid: Pid, sig: i32) -> LinuxResult<i32> {
    inc_ops();

    if tid <= 0 {
        return Err(LinuxError::EINVAL);
    }

    send_signal_to_thread(tid, sig)
}

/// tgkill - send signal to thread in thread group
pub fn tgkill(tgid: Pid, tid: Pid, sig: i32) -> LinuxResult<i32> {
    inc_ops();

    if tgid <= 0 || tid <= 0 {
        return Err(LinuxError::EINVAL);
    }

    if sig < 0 || sig > 64 {
        return Err(LinuxError::EINVAL);
    }

    let process_manager = process::get_process_manager();
    let pcb = process_manager
        .get_process(tid as u32)
        .ok_or(LinuxError::ESRCH)?;

    if pcb.pid as Pid != tgid && pcb.pgid as Pid != tgid {
        return Err(LinuxError::ESRCH);
    }

    send_signal_to_thread(tid, sig)
}

// ============================================================================
// Futex Operations
// ============================================================================

/// futex - fast userspace mutex
pub fn futex(
    uaddr: *mut i32,
    futex_op: i32,
    val: i32,
    _timeout: *const TimeSpec,
    _uaddr2: *mut i32,
    _val3: i32,
) -> LinuxResult<i32> {
    inc_ops();

    if uaddr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Strip the flag bits before matching the operation. FUTEX_CLOCK_REALTIME
    // only selects the clock used for the (currently best-effort) timeout, so
    // a CLOCK_REALTIME-flagged FUTEX_WAIT_BITSET still dispatches correctly.
    let op = futex_op & !(futex_op::FUTEX_PRIVATE_FLAG | futex_op::FUTEX_CLOCK_REALTIME);

    match op {
        futex_op::FUTEX_WAIT => {
            unsafe {
                if *uaddr != val {
                    return Err(LinuxError::EAGAIN);
                }
            }

            let key = uaddr as usize;
            let pid = process::current_pid();
            {
                let mut waiters = FUTEX_WAITERS.write();
                waiters.entry(key).or_default().push(FutexWaiter {
                    pid: pid as Pid,
                    bitset: futex_op::FUTEX_BITSET_MATCH_ANY,
                    pi_requeue_target: 0,
                });
                // Mark ourselves Blocked while still holding the queue lock. A
                // concurrent FUTEX_WAKE must take this same lock to drain us, so
                // it cannot deliver the wakeup before we have transitioned to
                // Blocked — closing the lost-wakeup race. block_process only
                // updates state and the scheduler queue (it does not yield), so
                // holding the lock across it is deadlock-free.
                let _ = process::get_process_manager().block_process(pid);
            }

            // A return from block_process means we were woken — normally by
            // FUTEX_WAKE, which already drained our entry. Report success.
            // Re-checking *uaddr and returning EAGAIN here would spuriously
            // fail the normal wake path, since the waker just changed the word.
            Ok(0)
        }
        futex_op::FUTEX_WAKE => {
            let key = uaddr as usize;
            let mut woke = 0i32;
            if val > 0 {
                let mut waiters = FUTEX_WAITERS.write();
                if let Some(queue) = waiters.get_mut(&key) {
                    let count = core::cmp::min(val as usize, queue.len());
                    for waiter in queue.drain(..count) {
                        let _ = process::get_process_manager().unblock_process(waiter.pid as u32);
                        woke += 1;
                    }
                    if queue.is_empty() {
                        waiters.remove(&key);
                    }
                }
            }
            Ok(woke)
        }
        futex_op::FUTEX_REQUEUE => {
            // Wake up to `val` waiters, then move up to val2 waiters from uaddr to uaddr2.
            if _uaddr2.is_null() {
                return Err(LinuxError::EFAULT);
            }
            let key = uaddr as usize;
            let key2 = _uaddr2 as usize;
            let nr_wake = val.max(0) as usize;
            let nr_requeue = _timeout as usize;
            let mut woke = 0i32;
            let mut moved = 0i32;
            let mut waiters = FUTEX_WAITERS.write();
            if let Some(queue) = waiters.get_mut(&key) {
                let count = core::cmp::min(nr_wake, queue.len());
                for waiter in queue.drain(..count) {
                    let _ = process::get_process_manager().unblock_process(waiter.pid as u32);
                    woke += 1;
                }
            }
            let drained: Vec<FutexWaiter> = {
                let queue = match waiters.get_mut(&key) {
                    Some(q) if !q.is_empty() => q,
                    _ => return Ok(woke),
                };
                let count = core::cmp::min(nr_requeue, queue.len());
                queue.drain(..count).collect()
            };
            if waiters.get(&key).map(|q| q.is_empty()).unwrap_or(true) {
                waiters.remove(&key);
            }
            for waiter in drained {
                waiters.entry(key2).or_default().push(waiter);
                moved += 1;
            }
            Ok(woke + moved)
        }
        futex_op::FUTEX_CMP_REQUEUE => {
            // Like REQUEUE but only if *uaddr still equals val3.
            if _uaddr2.is_null() {
                return Err(LinuxError::EFAULT);
            }
            unsafe {
                if *uaddr != _val3 {
                    return Err(LinuxError::EAGAIN);
                }
            }
            let key = uaddr as usize;
            let key2 = _uaddr2 as usize;
            let nr_wake = val.max(0) as usize;
            let nr_requeue = _timeout as usize;
            let mut woke = 0i32;
            let mut moved = 0i32;
            let mut waiters = FUTEX_WAITERS.write();
            if let Some(queue) = waiters.get_mut(&key) {
                let count = core::cmp::min(nr_wake, queue.len());
                for waiter in queue.drain(..count) {
                    let _ = process::get_process_manager().unblock_process(waiter.pid as u32);
                    woke += 1;
                }
            }
            let drained: Vec<FutexWaiter> = {
                let queue = match waiters.get_mut(&key) {
                    Some(q) if !q.is_empty() => q,
                    _ => return Ok(woke),
                };
                let count = core::cmp::min(nr_requeue, queue.len());
                queue.drain(..count).collect()
            };
            if waiters.get(&key).map(|q| q.is_empty()).unwrap_or(true) {
                waiters.remove(&key);
            }
            for waiter in drained {
                waiters.entry(key2).or_default().push(waiter);
                moved += 1;
            }
            Ok(woke + moved)
        }
        futex_op::FUTEX_WAIT_BITSET => {
            // Like FUTEX_WAIT but the waiter records a bitset; a zero bitset is
            // invalid. `val3` carries the mask. The timeout is absolute rather
            // than relative, but timeouts are still best-effort here.
            let bitset = _val3 as u32;
            if bitset == 0 {
                return Err(LinuxError::EINVAL);
            }
            unsafe {
                if *uaddr != val {
                    return Err(LinuxError::EAGAIN);
                }
            }

            let key = uaddr as usize;
            let pid = process::current_pid();
            {
                let mut waiters = FUTEX_WAITERS.write();
                waiters.entry(key).or_default().push(FutexWaiter {
                    pid: pid as Pid,
                    bitset,
                    pi_requeue_target: 0,
                });
                // Block under the queue lock to close the lost-wakeup race with
                // a concurrent FUTEX_WAKE_BITSET (see FUTEX_WAIT above).
                let _ = process::get_process_manager().block_process(pid);
            }

            // Woken (normally by FUTEX_WAKE_BITSET). Report success rather than
            // re-checking *uaddr, which would spuriously return EAGAIN.
            Ok(0)
        }
        futex_op::FUTEX_WAKE_BITSET => {
            // Like FUTEX_WAKE but only wakes waiters whose bitset intersects
            // `val3`. A zero bitset is invalid.
            let bitset = _val3 as u32;
            if bitset == 0 {
                return Err(LinuxError::EINVAL);
            }
            let key = uaddr as usize;
            let mut woke = 0i32;
            if val > 0 {
                let nr_wake = val as usize;
                let mut waiters = FUTEX_WAITERS.write();
                if let Some(queue) = waiters.get_mut(&key) {
                    // Retain non-matching waiters; wake (and remove) up to
                    // nr_wake matching ones, preserving FIFO order.
                    let mut remaining = Vec::with_capacity(queue.len());
                    for waiter in queue.drain(..) {
                        if woke < nr_wake as i32 && (waiter.bitset & bitset) != 0 {
                            let _ =
                                process::get_process_manager().unblock_process(waiter.pid as u32);
                            woke += 1;
                        } else {
                            remaining.push(waiter);
                        }
                    }
                    if remaining.is_empty() {
                        waiters.remove(&key);
                    } else {
                        *queue = remaining;
                    }
                }
            }
            Ok(woke)
        }
        futex_op::FUTEX_LOCK_PI => {
            // FUTEX_LOCK_PI: Lock a PI futex. If the futex word is 0 (unlocked),
            // atomically set it to our TID. Otherwise, wait.
            let pid = process::current_pid() as i32;
            let futex_word = unsafe { &*(uaddr as *const AtomicI32) };
            if futex_word
                .compare_exchange(0, pid, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Ok(0);
            }
            // Slow path: wait like FUTEX_WAIT but don't check val
            let key = uaddr as usize;
            {
                let mut waiters = FUTEX_WAITERS.write();
                waiters.entry(key).or_default().push(FutexWaiter {
                    pid: pid as Pid,
                    bitset: futex_op::FUTEX_BITSET_MATCH_ANY,
                    pi_requeue_target: 0,
                });
                // Block under the queue lock to close the lost-wakeup race
                // against FUTEX_UNLOCK_PI (see FUTEX_WAIT above).
                let _ = process::get_process_manager().block_process(pid as u32);
            }
            // When woken, try to acquire again
            let _ = futex_word.compare_exchange(0, pid, Ordering::SeqCst, Ordering::SeqCst);
            Ok(0)
        }
        futex_op::FUTEX_UNLOCK_PI => {
            // FUTEX_UNLOCK_PI: Unlock a PI futex. Atomically set to 0 and wake
            // one waiter.
            let pid = process::current_pid() as i32;
            let futex_word = unsafe { &*(uaddr as *const AtomicI32) };
            if futex_word
                .compare_exchange(pid, 0, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return Err(LinuxError::EPERM);
            }
            let key = uaddr as usize;
            let mut woke = 0i32;
            let mut waiters = FUTEX_WAITERS.write();
            if let Some(queue) = waiters.get_mut(&key) {
                if let Some(waiter) = queue.first().copied() {
                    let _ = process::get_process_manager().unblock_process(waiter.pid as u32);
                    queue.remove(0);
                    woke = 1;
                }
                if queue.is_empty() {
                    waiters.remove(&key);
                }
            }
            Ok(woke)
        }
        futex_op::FUTEX_TRYLOCK_PI => {
            // FUTEX_TRYLOCK_PI: Try to lock a PI futex without waiting.
            let pid = process::current_pid() as i32;
            let futex_word = unsafe { &*(uaddr as *const AtomicI32) };
            if futex_word
                .compare_exchange(0, pid, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                Ok(0)
            } else {
                Err(LinuxError::EAGAIN)
            }
        }
        futex_op::FUTEX_WAIT_REQUEUE_PI => {
            // FUTEX_WAIT_REQUEUE_PI: Wait on uaddr (a regular futex) with the
            // expectation that a FUTEX_CMP_REQUEUE_PI will requeue us to
            // uaddr2 (a PI futex).  When requeued and woken, we acquire uaddr2.
            //
            // Parameters: uaddr = source futex, val = expected value,
            // uaddr2 = target PI futex, val3 = expected value of uaddr2.
            if _uaddr2.is_null() {
                return Err(LinuxError::EFAULT);
            }

            // Check that *uaddr still equals val (like FUTEX_WAIT).
            unsafe {
                if *uaddr != val {
                    return Err(LinuxError::EAGAIN);
                }
            }

            let key = uaddr as usize;
            let target_key = _uaddr2 as usize;
            let pid = process::current_pid();

            {
                let mut waiters = FUTEX_WAITERS.write();
                waiters.entry(key).or_default().push(FutexWaiter {
                    pid: pid as Pid,
                    bitset: futex_op::FUTEX_BITSET_MATCH_ANY,
                    pi_requeue_target: target_key,
                });
                // Block under the queue lock to close the lost-wakeup race.
                let _ = process::get_process_manager().block_process(pid);
            }
            crate::scheduler::yield_cpu();

            // When woken, we've been requeued to uaddr2 and it's our turn.
            // Try to acquire the PI futex at uaddr2 by atomically setting it
            // to our TID (if it's 0).
            let pi_futex = unsafe { &*(_uaddr2 as *const AtomicI32) };
            let tid = pid as i32;
            pi_futex
                .compare_exchange(0, tid, Ordering::SeqCst, Ordering::SeqCst)
                .map(|_| 0)
                .map_err(|_| LinuxError::EAGAIN)
        }
        futex_op::FUTEX_CMP_REQUEUE_PI => {
            // FUTEX_CMP_REQUEUE_PI: Wake up to `val` waiters on uaddr and
            // requeue them onto the PI futex at uaddr2.  Like FUTEX_CMP_REQUEUE
            // but the requeued waiters become PI futex waiters.
            //
            // Parameters: uaddr = source, val = nr_wake, uaddr2 = target PI futex,
            // val3 = expected value of *uaddr (cmp check).
            if _uaddr2.is_null() {
                return Err(LinuxError::EFAULT);
            }

            // CMP check: *uaddr must equal val3, otherwise EAGAIN.
            unsafe {
                if *uaddr != _val3 {
                    return Err(LinuxError::EAGAIN);
                }
            }

            let key = uaddr as usize;
            let target_key = _uaddr2 as usize;
            let nr_wake = val.max(0) as usize;
            let nr_requeue = (_timeout as usize).max(0);
            let mut woke = 0i32;
            let mut requeued = 0i32;

            let mut waiters = FUTEX_WAITERS.write();

            // Wake up to nr_wake waiters that have pi_requeue_target == target_key.
            // Requeue up to nr_requeue more onto the target PI futex wait queue.
            let mut requeued_waiters: Vec<FutexWaiter> = Vec::new();
            if let Some(queue) = waiters.get_mut(&key) {
                let mut remaining: Vec<FutexWaiter> = Vec::with_capacity(queue.len());
                for waiter in queue.drain(..) {
                    if waiter.pi_requeue_target == target_key {
                        if woke < nr_wake as i32 {
                            requeued_waiters.push(FutexWaiter {
                                pid: waiter.pid,
                                bitset: futex_op::FUTEX_BITSET_MATCH_ANY,
                                pi_requeue_target: 0,
                            });
                            woke += 1;
                        } else if (requeued as usize) < nr_requeue {
                            requeued_waiters.push(FutexWaiter {
                                pid: waiter.pid,
                                bitset: futex_op::FUTEX_BITSET_MATCH_ANY,
                                pi_requeue_target: 0,
                            });
                            requeued += 1;
                        } else {
                            remaining.push(waiter);
                        }
                    } else {
                        remaining.push(waiter);
                    }
                }
                if remaining.is_empty() {
                    waiters.remove(&key);
                } else {
                    *queue = remaining;
                }
            }
            for waiter in requeued_waiters {
                let pid = waiter.pid;
                waiters.entry(target_key).or_default().push(waiter);
                let _ = process::get_process_manager().unblock_process(pid as u32);
            }

            Ok(woke + requeued)
        }
        _ => Err(LinuxError::ENOSYS),
    }
}

/// robust_list_head for futex robustness
#[repr(C)]
pub struct RobustListHead {
    pub list: *mut RobustList,
    pub futex_offset: i64,
    pub list_op_pending: *mut RobustList,
}

#[repr(C)]
pub struct RobustList {
    pub next: *mut RobustList,
}

/// set_robust_list - set robust futex list
pub fn set_robust_list(head: *mut RobustListHead, len: usize) -> LinuxResult<i32> {
    inc_ops();

    if head.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if len != core::mem::size_of::<RobustListHead>() {
        return Err(LinuxError::EINVAL);
    }

    ROBUST_LISTS.write().insert(current_tid(), head as usize);
    Ok(0)
}

/// get_robust_list - get robust futex list
pub fn get_robust_list(
    pid: Pid,
    head_ptr: *mut *mut RobustListHead,
    len_ptr: *mut usize,
) -> LinuxResult<i32> {
    inc_ops();

    if head_ptr.is_null() || len_ptr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let target = if pid == 0 { current_tid() } else { pid };
    process::get_process_manager()
        .get_process(target as u32)
        .ok_or(LinuxError::ESRCH)?;

    let head_addr = ROBUST_LISTS.read().get(&target).copied().unwrap_or(0);
    copy_u64_to_user_addr(head_ptr as u64, head_addr as u64)?;
    copy_usize_to_user(len_ptr, core::mem::size_of::<RobustListHead>())?;
    Ok(0)
}

// ============================================================================
// Thread-Local Storage
// ============================================================================

/// set_thread_area - set a GDT entry for thread-local storage (x86)
pub fn set_thread_area(u_info: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if u_info.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let mut area = [0u8; 32];
    unsafe {
        core::ptr::copy_nonoverlapping(u_info, area.as_mut_ptr(), 32);
    }
    THREAD_AREAS.write().insert(current_tid(), area);
    Ok(0)
}

/// get_thread_area - get a GDT entry for thread-local storage (x86)
pub fn get_thread_area(u_info: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if u_info.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let tid = current_tid();
    let area = THREAD_AREAS.read().get(&tid).copied().unwrap_or([0u8; 32]);
    unsafe {
        core::ptr::copy_nonoverlapping(area.as_ptr(), u_info, 32);
    }
    Ok(0)
}

/// arch_prctl - set architecture-specific thread state
pub fn arch_prctl(code: i32, addr: u64) -> LinuxResult<i32> {
    inc_ops();

    use crate::arch::x86::prctl;

    let tid = current_tid();

    match code {
        prctl::ARCH_SET_FS => {
            TLS_FS_BASE.write().insert(tid, addr);
            // Update the PCB's fs_base so context switches save/restore it.
            let pm = crate::process::get_process_manager();
            let pid = pm.current_process();
            let _ = pm.set_fs_base(pid, addr);
            unsafe {
                wrmsr(MSR_FS_BASE, addr);
            }
            Ok(0)
        }
        prctl::ARCH_GET_FS => {
            if addr == 0 {
                return Err(LinuxError::EFAULT);
            }
            let base = TLS_FS_BASE.read().get(&tid).copied().unwrap_or(0);
            copy_u64_to_user_addr(addr, base)?;
            Ok(0)
        }
        prctl::ARCH_SET_GS => {
            TLS_GS_BASE.write().insert(tid, addr);
            unsafe {
                wrmsr(MSR_GS_BASE, addr);
            }
            Ok(0)
        }
        prctl::ARCH_GET_GS => {
            if addr == 0 {
                return Err(LinuxError::EFAULT);
            }
            let base = TLS_GS_BASE.read().get(&tid).copied().unwrap_or(0);
            copy_u64_to_user_addr(addr, base)?;
            Ok(0)
        }
        prctl::ARCH_GET_CPUID => {
            if addr == 0 {
                return Err(LinuxError::EFAULT);
            }
            copy_u64_to_user_addr(addr, 1)?;
            Ok(0)
        }
        prctl::ARCH_SET_CPUID => {
            if addr != 0 && addr != 1 {
                return Err(LinuxError::EINVAL);
            }
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

// ============================================================================
// CPU Affinity
// ============================================================================

/// CPU set type
pub type CpuSet = u64;

/// sched_setaffinity - set CPU affinity
pub fn sched_setaffinity(pid: Pid, cpusetsize: usize, mask: *const CpuSet) -> LinuxResult<i32> {
    inc_ops();

    if mask.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if cpusetsize == 0 {
        return Err(LinuxError::EINVAL);
    }

    let cpu_mask = copy_u64_from_user_addr(mask as u64)?;
    if cpu_mask == 0 {
        return Err(LinuxError::EINVAL);
    }

    let target_pid = if pid == 0 {
        process::current_pid()
    } else {
        pid as u32
    };

    process::get_process_manager()
        .with_process_mut(target_pid, |pcb| {
            pcb.sched_info.cpu_affinity = cpu_mask;
        })
        .ok_or(LinuxError::ESRCH)?;

    Ok(0)
}

/// sched_getaffinity - get CPU affinity
pub fn sched_getaffinity(_pid: Pid, cpusetsize: usize, mask: *mut CpuSet) -> LinuxResult<i32> {
    inc_ops();

    if mask.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if cpusetsize == 0 {
        return Err(LinuxError::EINVAL);
    }

    let target_pid = if _pid == 0 {
        process::current_pid()
    } else {
        _pid as u32
    };

    let cpu_affinity = process::get_process_manager()
        .get_process(target_pid)
        .map(|pcb| pcb.sched_info.cpu_affinity)
        .ok_or(LinuxError::ESRCH)?;

    copy_u64_to_user_addr(mask as u64, cpu_affinity)?;
    Ok(0)
}

// ============================================================================
// Thread Exit
// ============================================================================

/// exit - terminate current thread
pub fn exit(status: i32) -> ! {
    inc_ops();
    process_ops::exit(status)
}

/// exit_group - terminate all threads in process
pub fn exit_group(status: i32) -> ! {
    inc_ops();
    process_ops::exit(status)
}

// ============================================================================
// Barriers
// ============================================================================

/// membarrier - issue memory barriers on set of threads
pub fn membarrier(cmd: i32, flags: i32) -> LinuxResult<i32> {
    inc_ops();

    const MEMBARRIER_CMD_QUERY: i32 = 0;
    const MEMBARRIER_CMD_GLOBAL: i32 = 1 << 0;
    const MEMBARRIER_CMD_GLOBAL_EXPEDITED: i32 = 1 << 1;
    const MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED: i32 = 1 << 2;
    const MEMBARRIER_CMD_PRIVATE_EXPEDITED: i32 = 1 << 3;
    const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED: i32 = 1 << 4;
    const MEMBARRIER_CMD_GET_REGISTRATIONS: i32 = 1 << 9;
    const MEMBARRIER_SUPPORTED: i32 = MEMBARRIER_CMD_GLOBAL
        | MEMBARRIER_CMD_GLOBAL_EXPEDITED
        | MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED
        | MEMBARRIER_CMD_PRIVATE_EXPEDITED
        | MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED
        | MEMBARRIER_CMD_GET_REGISTRATIONS;

    match cmd {
        MEMBARRIER_CMD_QUERY => Ok(MEMBARRIER_SUPPORTED),
        MEMBARRIER_CMD_GET_REGISTRATIONS => {
            if flags != 0 {
                return Err(LinuxError::EINVAL);
            }
            Ok(MEMBARRIER_REGISTRATIONS.load(Ordering::SeqCst))
        }
        MEMBARRIER_CMD_GLOBAL
        | MEMBARRIER_CMD_GLOBAL_EXPEDITED
        | MEMBARRIER_CMD_PRIVATE_EXPEDITED => {
            if flags != 0 {
                return Err(LinuxError::EINVAL);
            }
            core::sync::atomic::fence(Ordering::SeqCst);
            core::sync::atomic::compiler_fence(Ordering::SeqCst);
            Ok(0)
        }
        MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED | MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED => {
            if flags != 0 {
                return Err(LinuxError::EINVAL);
            }
            MEMBARRIER_REGISTRATIONS.fetch_or(cmd, Ordering::SeqCst);
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// clone3 - create a child process or thread with extended arguments
pub fn clone3(cl_args: *const CloneArgs, size: usize) -> LinuxResult<Pid> {
    inc_ops();

    if cl_args.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let expected_size = core::mem::size_of::<CloneArgs>();
    if size < expected_size {
        return Err(LinuxError::EINVAL);
    }

    let args = unsafe { &*cl_args };

    if (args.flags & clone_flags::CLONE_THREAD) != 0 {
        return Err(LinuxError::EINVAL);
    }

    clone(
        args.flags,
        args.stack as *mut u8,
        args.parent_tid as *mut Pid,
        args.child_tid as *mut Pid,
        args.tls,
    )
}

/// getcpu - determine current CPU and NUMA node
///
/// Reads the actual CPU ID from the APIC via `smp::current_cpu()` and
/// maps it to a NUMA node via `smp::get_cpu_data()`.  The tcache pointer
/// is ignored (Linux also deprecated it).
pub fn getcpu(cpu: *mut u32, node: *mut u32, _tcache: *mut u8) -> LinuxResult<i32> {
    inc_ops();
    let cpu_id = crate::smp::current_cpu();
    let node_id = crate::smp::get_cpu_data(cpu_id)
        .map(|_| 0u32) // Single NUMA node for now; smp doesn't track NUMA yet
        .unwrap_or(0);

    if !cpu.is_null() {
        // SAFETY: caller guarantees cpu points to a valid, writable u32.
        unsafe { *cpu = cpu_id };
    }
    if !node.is_null() {
        // SAFETY: caller guarantees node points to a valid, writable u32.
        unsafe { *node = node_id };
    }
    Ok(0)
}

#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_clone_flags() {
        let flags = clone_flags::CLONE_THREAD | clone_flags::CLONE_VM | clone_flags::CLONE_SIGHAND;
        assert!(clone(
            flags,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            0
        )
        .is_ok());
    }

    #[test_case]
    fn test_futex_wait() {
        let mut futex_word: i32 = 0;

        assert_eq!(
            futex(
                &mut futex_word,
                futex_op::FUTEX_WAIT,
                1,
                core::ptr::null(),
                core::ptr::null_mut(),
                0
            ),
            Err(LinuxError::EAGAIN)
        );
    }

    #[test_case]
    fn test_futex_wake() {
        let mut futex_word: i32 = 0;

        assert!(futex(
            &mut futex_word,
            futex_op::FUTEX_WAKE,
            1,
            core::ptr::null(),
            core::ptr::null_mut(),
            0
        )
        .is_ok());
    }

    #[test_case]
    fn test_gettid() {
        let tid = gettid();
        assert!(tid > 0);
    }

    #[test_case]
    fn test_cpu_affinity() {
        let mut mask: CpuSet = 0;
        assert!(sched_getaffinity(0, 8, &mut mask).is_ok());
        assert!(sched_setaffinity(0, 8, &mask).is_ok());
    }
}
