//! Threading and synchronization operations
//!
//! This module implements Linux threading operations including
//! futex, clone, thread-local storage, and pthread-compatible functions.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

use super::process_ops;
use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::process;

/// FS base MSR (x86_64)
const MSR_FS_BASE: u32 = 0xC000_0100;
/// GS base MSR (x86_64)
const MSR_GS_BASE: u32 = 0xC000_0101;

static CLEAR_CHILD_TID: AtomicU64 = AtomicU64::new(0);

/// Per-thread FS/GS base addresses keyed by tid.
static TLS_FS_BASE: RwLock<BTreeMap<Pid, u64>> = RwLock::new(BTreeMap::new());
static TLS_GS_BASE: RwLock<BTreeMap<Pid, u64>> = RwLock::new(BTreeMap::new());

/// Per-thread robust futex list heads keyed by tid (stored as address).
static ROBUST_LISTS: RwLock<BTreeMap<Pid, usize>> = RwLock::new(BTreeMap::new());

/// Legacy x86 thread area entries keyed by tid.
static THREAD_AREAS: RwLock<BTreeMap<Pid, [u8; 32]>> = RwLock::new(BTreeMap::new());

/// Futex wait queues keyed by userspace address.
static FUTEX_WAITERS: RwLock<BTreeMap<usize, Vec<Pid>>> = RwLock::new(BTreeMap::new());

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
}

// ============================================================================
// Clone and Thread Creation
// ============================================================================

/// clone - create a child process or thread
pub fn clone(
    flags: u64,
    _stack: *mut u8,
    _parent_tid: *mut Pid,
    _child_tid: *mut Pid,
    _tls: u64,
) -> LinuxResult<Pid> {
    inc_ops();

    if (flags & clone_flags::CLONE_THREAD) != 0 {
        return process_ops::fork();
    }

    process_ops::fork()
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

    let op = futex_op & !futex_op::FUTEX_PRIVATE_FLAG;

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
                waiters.entry(key).or_default().push(pid as Pid);
            }

            let _ = process::get_process_manager().block_process(pid);

            unsafe {
                if *uaddr != val {
                    return Err(LinuxError::EAGAIN);
                }
            }
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
                        let _ = process::get_process_manager().unblock_process(waiter as u32);
                        woke += 1;
                    }
                    if queue.is_empty() {
                        waiters.remove(&key);
                    }
                }
            }
            Ok(woke)
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
    unsafe {
        *head_ptr = head_addr as *mut RobustListHead;
        *len_ptr = core::mem::size_of::<RobustListHead>();
    }
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

    const ARCH_SET_GS: i32 = 0x1001;
    const ARCH_SET_FS: i32 = 0x1002;
    const ARCH_GET_FS: i32 = 0x1003;
    const ARCH_GET_GS: i32 = 0x1004;

    let tid = current_tid();

    match code {
        ARCH_SET_FS => {
            TLS_FS_BASE.write().insert(tid, addr);
            unsafe {
                wrmsr(MSR_FS_BASE, addr);
            }
            Ok(0)
        }
        ARCH_GET_FS => {
            if addr == 0 {
                return Err(LinuxError::EFAULT);
            }
            let base = TLS_FS_BASE.read().get(&tid).copied().unwrap_or(0);
            unsafe {
                *(addr as *mut u64) = base;
            }
            Ok(0)
        }
        ARCH_SET_GS => {
            TLS_GS_BASE.write().insert(tid, addr);
            unsafe {
                wrmsr(MSR_GS_BASE, addr);
            }
            Ok(0)
        }
        ARCH_GET_GS => {
            if addr == 0 {
                return Err(LinuxError::EFAULT);
            }
            let base = TLS_GS_BASE.read().get(&tid).copied().unwrap_or(0);
            unsafe {
                *(addr as *mut u64) = base;
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

    let cpu_mask = unsafe { *mask };
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

    unsafe {
        *mask = cpu_affinity;
    }
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
pub fn membarrier(cmd: i32, _flags: i32) -> LinuxResult<i32> {
    inc_ops();

    const MEMBARRIER_CMD_QUERY: i32 = 0;
    const MEMBARRIER_CMD_GLOBAL: i32 = 1;
    const MEMBARRIER_CMD_PRIVATE_EXPEDITED: i32 = 2;

    match cmd {
        MEMBARRIER_CMD_QUERY => Ok(MEMBARRIER_CMD_GLOBAL | MEMBARRIER_CMD_PRIVATE_EXPEDITED),
        MEMBARRIER_CMD_GLOBAL => {
            core::sync::atomic::fence(Ordering::SeqCst);
            core::sync::atomic::compiler_fence(Ordering::SeqCst);
            Ok(0)
        }
        MEMBARRIER_CMD_PRIVATE_EXPEDITED => {
            core::sync::atomic::fence(Ordering::SeqCst);
            core::sync::atomic::compiler_fence(Ordering::SeqCst);
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
/// Single-core stub: always reports CPU 0, node 0.  This is sufficient for
/// userspace that only needs a stable cpu number for per-thread caching; the
/// tcache pointer is ignored.
pub fn getcpu(cpu: *mut u32, node: *mut u32, _tcache: *mut u8) -> LinuxResult<i32> {
    inc_ops();
    if !cpu.is_null() {
        // SAFETY: caller guarantees cpu points to a valid, writable u32.
        unsafe { *cpu = 0 };
    }
    if !node.is_null() {
        // SAFETY: caller guarantees node points to a valid, writable u32.
        unsafe { *node = 0 };
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
