//! Futex — Fast Userspace Mutexes
//!
//! Ported from Linux kernel/futex/core.c, syscalls.c, waitwake.c.
//! Provides kernel-side support for userspace synchronization primitives:
//! - FUTEX_WAIT / FUTEX_WAKE
//! - FUTEX_WAIT_BITSET / FUTEX_WAKE_BITSET
//! - FUTEX_REQUEUE / FUTEX_CMP_REQUEUE
//! - FUTEX_WAKE_OP
//! - Robust futex list handling
//!
//! The key insight is that waiters register a (uaddr, pid) pair in a hash
//! table. Wakers scan the hash bucket and unblock matching waiters.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, RwLock};

// ── Futex op constants (from Linux uapi) ────────────────────────────────

pub const FUTEX_WAIT: i32 = 0;
pub const FUTEX_WAKE: i32 = 1;
pub const FUTEX_FD: i32 = 2;
pub const FUTEX_REQUEUE: i32 = 3;
pub const FUTEX_CMP_REQUEUE: i32 = 4;
pub const FUTEX_WAKE_OP: i32 = 5;
pub const FUTEX_LOCK_PI: i32 = 6;
pub const FUTEX_UNLOCK_PI: i32 = 7;
pub const FUTEX_TRYLOCK_PI: i32 = 8;
pub const FUTEX_WAIT_BITSET: i32 = 9;
pub const FUTEX_WAKE_BITSET: i32 = 10;
pub const FUTEX_WAIT_REQUEUE_PI: i32 = 11;
pub const FUTEX_CMP_REQUEUE_PI: i32 = 12;
pub const FUTEX_LOCK_PI2: i32 = 13;

pub const FUTEX_PRIVATE_FLAG: i32 = 128;
pub const FUTEX_CLOCK_REALTIME: i32 = 256;

pub const FUTEX_BITSET_MATCH_ANY: u32 = 0xFFFFFFFF;

// FUTEX_WAKE_OP op/cmp codes
const FUTEX_OP_SET: i32 = 0;
const FUTEX_OP_ADD: i32 = 1;
const FUTEX_OP_OR: i32 = 2;
const FUTEX_OP_ANDN: i32 = 3;
const FUTEX_OP_XOR: i32 = 4;

const FUTEX_OP_OPARG_SHIFT: i32 = 8;

const FUTEX_OP_CMP_EQ: i32 = 0;
const FUTEX_OP_CMP_NE: i32 = 1;
const FUTEX_OP_CMP_LT: i32 = 2;
const FUTEX_OP_CMP_LE: i32 = 3;
const FUTEX_OP_CMP_GT: i32 = 4;
const FUTEX_OP_CMP_GE: i32 = 5;

// ── Hash table ──────────────────────────────────────────────────────────

const FUTEX_HASH_SIZE: usize = 256;

fn futex_hash(uaddr: usize) -> usize {
    // Simple hash mixing — good enough for our purposes
    let mut h = uaddr as u64;
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD5_ED82_8D49);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    (h as usize) & (FUTEX_HASH_SIZE - 1)
}

// ── Waiter representation ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct FutexWaiter {
    pid: u32,
    tid: u32,
    bitset: u32,
}

// ── Hash bucket ─────────────────────────────────────────────────────────

struct FutexBucket {
    waiters: Vec<(usize, FutexWaiter)>, // (uaddr, waiter)
}

impl FutexBucket {
    const fn new() -> Self {
        Self {
            waiters: Vec::new(),
        }
    }
}

static FUTEX_BUCKETS: [Mutex<FutexBucket>; FUTEX_HASH_SIZE] = {
    const B: Mutex<FutexBucket> = Mutex::new(FutexBucket::new());
    [B; FUTEX_HASH_SIZE]
};

// ── Robust futex list ───────────────────────────────────────────────────

#[repr(C)]
pub struct RobustList {
    pub next: *mut RobustList,
}

#[repr(C)]
pub struct RobustListHead {
    pub list: *mut RobustList,
    pub futex_offset: i64,
    pub list_op_pending: *mut RobustList,
}

/// Wrapper to make raw pointer Send+Sync for static storage.
/// Safety: the pointer is only accessed under the RwLock guard.
struct SyncPtr(*mut RobustListHead);
unsafe impl Send for SyncPtr {}
unsafe impl Sync for SyncPtr {}

static ROBUST_LISTS: RwLock<BTreeMap<u32, SyncPtr>> = RwLock::new(BTreeMap::new());

// ── Statistics ──────────────────────────────────────────────────────────

static FUTEX_STATS_WAIT: AtomicU64 = AtomicU64::new(0);
static FUTEX_STATS_WAKE: AtomicU64 = AtomicU64::new(0);
static FUTEX_STATS_REQUEUE: AtomicU64 = AtomicU64::new(0);
static FUTEX_STATS_WAKE_OP: AtomicU64 = AtomicU64::new(0);

// ── Timeout helper ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct FutexTimeout {
    pub abs_time_ns: u64,
    pub clock_realtime: bool,
}

impl FutexTimeout {
    pub fn from_timespec(ts: &crate::linux_compat::TimeSpec, realtime: bool) -> Self {
        let ns = ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64;
        // If realtime, treat as absolute uptime deadline (no realtime clock yet)
        let abs = if realtime {
            ns
        } else {
            crate::time::uptime_ns() + ns
        };
        Self {
            abs_time_ns: abs,
            clock_realtime: realtime,
        }
    }

    pub fn expired(&self) -> bool {
        let now = crate::time::uptime_ns();
        now >= self.abs_time_ns
    }

    pub fn remaining_ms(&self) -> i32 {
        let now = crate::time::uptime_ns();
        if self.abs_time_ns <= now {
            return 0;
        }
        let remaining = self.abs_time_ns - now;
        ((remaining / 1_000_000) as i32).min(i32::MAX as i32) as i32
    }
}

// ── Core futex operations ───────────────────────────────────────────────

/// FUTEX_WAIT — block on a futex if *uaddr == val
pub fn futex_wait(uaddr: *mut i32, val: i32, bitset: u32, timeout: Option<&FutexTimeout>) -> i32 {
    if uaddr.is_null() {
        return -14; // EFAULT
    }

    // Check current value atomically
    let current_val = unsafe { core::ptr::read_volatile(uaddr) };
    if current_val != val {
        return -11; // EAGAIN
    }

    let key = uaddr as usize;
    let hash = futex_hash(key);
    let pid = crate::process::current_pid();
    let tid = crate::process::thread::get_thread_manager().current_thread();

    // Register as waiter
    {
        let bucket = &FUTEX_BUCKETS[hash];
        let mut b = bucket.lock();
        b.waiters.push((key, FutexWaiter { pid, tid, bitset }));
    }

    FUTEX_STATS_WAIT.fetch_add(1, Ordering::Relaxed);

    // Block the current process/thread
    let pm = crate::process::get_process_manager();
    let _ = pm.block_process(pid);

    // After waking, check if we were woken by a wake or a timeout/spurious
    let current_val = unsafe { core::ptr::read_volatile(uaddr) };
    if current_val != val {
        // Value changed — remove ourselves if still in bucket
        let bucket = &FUTEX_BUCKETS[hash];
        let mut b = bucket.lock();
        b.waiters
            .retain(|(k, w)| !(*k == key && w.pid == pid && w.tid == tid));
        return -11; // EAGAIN
    }

    // Check timeout
    if let Some(to) = timeout {
        if to.expired() {
            let bucket = &FUTEX_BUCKETS[hash];
            let mut b = bucket.lock();
            b.waiters
                .retain(|(k, w)| !(*k == key && w.pid == pid && w.tid == tid));
            return -110; // ETIMEDOUT
        }
    }

    0
}

/// FUTEX_WAKE — wake up to `val` waiters on the futex at `uaddr`
pub fn futex_wake(uaddr: *mut i32, val: i32, bitset: u32) -> i32 {
    if uaddr.is_null() {
        return -14; // EFAULT
    }

    let key = uaddr as usize;
    let hash = futex_hash(key);
    let mut woken = 0i32;

    {
        let bucket = &FUTEX_BUCKETS[hash];
        let mut b = bucket.lock();

        let mut to_wake: Vec<usize> = Vec::new();
        for (i, (k, w)) in b.waiters.iter().enumerate() {
            if *k == key && (w.bitset & bitset) != 0 {
                to_wake.push(i);
                if to_wake.len() >= val as usize {
                    break;
                }
            }
        }

        // Remove in reverse order to keep indices valid
        for &i in to_wake.iter().rev() {
            let (_, waiter) = b.waiters.remove(i);
            let pm = crate::process::get_process_manager();
            let _ = pm.unblock_process(waiter.pid);
            woken += 1;
        }
    }

    FUTEX_STATS_WAKE.fetch_add(1, Ordering::Relaxed);
    woken
}

/// FUTEX_REQUEUE — requeue waiters from uaddr to uaddr2
/// wakes `val` waiters, then requeues up to `val2` more to uaddr2
pub fn futex_requeue(
    uaddr: *mut i32,
    uaddr2: *mut i32,
    val: i32,
    val2: i32,
    cmpval: Option<i32>,
) -> i32 {
    if uaddr.is_null() || uaddr2.is_null() {
        return -14; // EFAULT
    }

    // If cmpval is provided, check that *uaddr == cmpval
    if let Some(cv) = cmpval {
        let current = unsafe { core::ptr::read_volatile(uaddr) };
        if current != cv {
            return -11; // EAGAIN
        }
    }

    let key1 = uaddr as usize;
    let key2 = uaddr2 as usize;
    let hash1 = futex_hash(key1);
    let hash2 = futex_hash(key2);

    let mut woken = 0i32;
    let mut requeued = 0i32;

    // Wake up to `val` waiters
    if val > 0 {
        let bucket = &FUTEX_BUCKETS[hash1];
        let mut b = bucket.lock();
        let mut to_wake: Vec<usize> = Vec::new();
        for (i, (k, _)) in b.waiters.iter().enumerate() {
            if *k == key1 {
                to_wake.push(i);
                if to_wake.len() >= val as usize {
                    break;
                }
            }
        }
        for &i in to_wake.iter().rev() {
            let (_, waiter) = b.waiters.remove(i);
            let pm = crate::process::get_process_manager();
            let _ = pm.unblock_process(waiter.pid);
            woken += 1;
        }
    }

    // Requeue up to `val2` more waiters to uaddr2
    if val2 > 0 && hash1 != hash2 {
        let bucket1 = &FUTEX_BUCKETS[hash1];
        let bucket2 = &FUTEX_BUCKETS[hash2];
        let mut b1 = bucket1.lock();
        let mut b2 = bucket2.lock();

        let mut to_requeue: Vec<usize> = Vec::new();
        for (i, (k, _)) in b1.waiters.iter().enumerate() {
            if *k == key1 {
                to_requeue.push(i);
                if to_requeue.len() >= val2 as usize {
                    break;
                }
            }
        }
        for &i in to_requeue.iter().rev() {
            let (_, waiter) = b1.waiters.remove(i);
            b2.waiters.push((key2, waiter));
            requeued += 1;
        }
    } else if val2 > 0 {
        // Same bucket — just re-key
        let bucket = &FUTEX_BUCKETS[hash1];
        let mut b = bucket.lock();
        let mut count = 0i32;
        for entry in b.waiters.iter_mut() {
            if entry.0 == key1 && count < val2 {
                entry.0 = key2;
                count += 1;
            }
        }
        requeued = count;
    }

    FUTEX_STATS_REQUEUE.fetch_add(1, Ordering::Relaxed);
    woken + requeued
}

/// FUTEX_WAKE_OP — wake waiters on uaddr, conditionally waking on uaddr2
/// based on a comparison operation.
pub fn futex_wake_op(uaddr: *mut i32, uaddr2: *mut i32, val: i32, val2: i32, wake_op: i32) -> i32 {
    if uaddr.is_null() || uaddr2.is_null() {
        return -14; // EFAULT
    }

    // Decode the wake_op parameter
    let op = (wake_op >> 28) & 0x7;
    let op_arg = (wake_op >> 12) & 0xFFF;
    let cmp = (wake_op >> 24) & 0xF;
    let cmp_arg = wake_op & 0xFFF;

    // Perform the atomic operation on uaddr2
    let old_val = unsafe { core::ptr::read_volatile(uaddr2) };
    let new_val = match op {
        FUTEX_OP_SET => op_arg as i32,
        FUTEX_OP_ADD => old_val.wrapping_add(op_arg as i32),
        FUTEX_OP_OR => old_val | op_arg as i32,
        FUTEX_OP_ANDN => old_val & !(op_arg as i32),
        FUTEX_OP_XOR => old_val ^ (op_arg as i32),
        _ => old_val,
    };
    unsafe {
        core::ptr::write_volatile(uaddr2, new_val);
    }

    // Check comparison condition
    let cmp_result = match cmp {
        FUTEX_OP_CMP_EQ => old_val == cmp_arg,
        FUTEX_OP_CMP_NE => old_val != cmp_arg,
        FUTEX_OP_CMP_LT => old_val < cmp_arg,
        FUTEX_OP_CMP_LE => old_val <= cmp_arg,
        FUTEX_OP_CMP_GT => old_val > cmp_arg,
        FUTEX_OP_CMP_GE => old_val >= cmp_arg,
        _ => false,
    };

    let mut woken = 0i32;

    // Wake up to `val` waiters on uaddr
    if val > 0 {
        woken += futex_wake(uaddr, val, FUTEX_BITSET_MATCH_ANY);
    }

    // If comparison matched, wake up to `val2` waiters on uaddr2
    if cmp_result && val2 > 0 {
        woken += futex_wake(uaddr2, val2, FUTEX_BITSET_MATCH_ANY);
    }

    FUTEX_STATS_WAKE_OP.fetch_add(1, Ordering::Relaxed);
    woken
}

// ── Top-level futex syscall handler ─────────────────────────────────────

/// Main futex syscall entry point. Dispatches to the appropriate operation.
pub fn do_futex(
    uaddr: *mut i32,
    futex_op: i32,
    val: i32,
    timeout: Option<&FutexTimeout>,
    uaddr2: *mut i32,
    val2: i32,
    val3: i32,
) -> i32 {
    let op = futex_op & !(FUTEX_PRIVATE_FLAG | FUTEX_CLOCK_REALTIME);

    match op {
        FUTEX_WAIT => futex_wait(uaddr, val, FUTEX_BITSET_MATCH_ANY, timeout),
        FUTEX_WAIT_BITSET => futex_wait(uaddr, val, val3 as u32, timeout),
        FUTEX_WAKE => futex_wake(uaddr, val, FUTEX_BITSET_MATCH_ANY),
        FUTEX_WAKE_BITSET => futex_wake(uaddr, val, val3 as u32),
        FUTEX_REQUEUE => futex_requeue(uaddr, uaddr2, val, val2, None),
        FUTEX_CMP_REQUEUE => futex_requeue(uaddr, uaddr2, val, val2, Some(val3)),
        FUTEX_WAKE_OP => futex_wake_op(uaddr, uaddr2, val, val2, val3),
        FUTEX_LOCK_PI | FUTEX_LOCK_PI2 => {
            // PI futexes — simplified: treat as regular wait
            futex_wait(uaddr, 0, FUTEX_BITSET_MATCH_ANY, timeout)
        }
        FUTEX_UNLOCK_PI => futex_wake(uaddr, 1, FUTEX_BITSET_MATCH_ANY),
        FUTEX_TRYLOCK_PI => {
            if uaddr.is_null() {
                return -14;
            }
            let v = unsafe { core::ptr::read_volatile(uaddr) };
            if v == 0 {
                unsafe {
                    core::ptr::write_volatile(uaddr, crate::process::current_pid() as i32);
                }
                0
            } else {
                -11 // EAGAIN
            }
        }
        _ => -38, // ENOSYS
    }
}

// ── Robust futex list ───────────────────────────────────────────────────

/// Set the robust futex list head for the current thread.
pub fn set_robust_list(head: *mut RobustListHead, len: usize) -> i32 {
    if head.is_null() {
        return -14; // EFAULT
    }
    if len != core::mem::size_of::<RobustListHead>() {
        return -22; // EINVAL
    }
    let tid = crate::process::thread::get_thread_manager().current_thread();
    ROBUST_LISTS.write().insert(tid, SyncPtr(head));
    0
}

/// Get the robust futex list head for a given thread.
pub fn get_robust_list(tid: u32, head_ptr: *mut *mut RobustListHead, len_ptr: *mut usize) -> i32 {
    if head_ptr.is_null() || len_ptr.is_null() {
        return -14; // EFAULT
    }

    let target_tid = if tid == 0 {
        crate::process::thread::get_thread_manager().current_thread()
    } else {
        tid
    };

    let lists = ROBUST_LISTS.read();
    let Some(&SyncPtr(head)) = lists.get(&target_tid) else {
        return -3; // ESRCH
    };

    unsafe {
        *head_ptr = head;
        *len_ptr = core::mem::size_of::<RobustListHead>();
    }
    0
}

/// Called when a thread exits — walk the robust list and mark held futexes
/// with FUTEX_OWNER_DIED, then wake any waiters.
pub fn exit_robust_list(tid: u32) {
    let head_ptr = {
        let lists = ROBUST_LISTS.read();
        lists.get(&tid).map(|&SyncPtr(p)| p)
    };

    let Some(head_ptr) = head_ptr else { return };
    if head_ptr.is_null() {
        return;
    }

    let head = unsafe { &*head_ptr };
    let mut entry = head.list;
    let futex_offset = head.futex_offset;

    // Walk the robust list
    while !entry.is_null() {
        let node = unsafe { &*entry };
        // The futex word is at entry + futex_offset
        let futex_addr = unsafe {
            let raw = entry as *const u8;
            raw.offset(futex_offset as isize) as *mut i32
        };
        if !futex_addr.is_null() {
            let val = unsafe { core::ptr::read_volatile(futex_addr) };
            // If this thread owns the futex, mark it as OWNER_DIED and wake
            if val & 0x7FFFFFFF == tid as i32 {
                let died_val = val | 0x40000000; // FUTEX_OWNER_DIED
                unsafe {
                    core::ptr::write_volatile(futex_addr, died_val);
                }
                futex_wake(futex_addr, 1, FUTEX_BITSET_MATCH_ANY);
            }
        }
        entry = node.next;
    }

    ROBUST_LISTS.write().remove(&tid);
}

// ── Initialization ──────────────────────────────────────────────────────

pub fn init() {
    crate::serial_println!("[futex] initialized ({} hash buckets)", FUTEX_HASH_SIZE);
}

// ── Debug / stats ───────────────────────────────────────────────────────

pub fn futex_stats() -> (u64, u64, u64, u64) {
    (
        FUTEX_STATS_WAIT.load(Ordering::Relaxed),
        FUTEX_STATS_WAKE.load(Ordering::Relaxed),
        FUTEX_STATS_REQUEUE.load(Ordering::Relaxed),
        FUTEX_STATS_WAKE_OP.load(Ordering::Relaxed),
    )
}

pub fn futex_waiter_count() -> usize {
    let mut total = 0;
    for i in 0..FUTEX_HASH_SIZE {
        total += FUTEX_BUCKETS[i].lock().waiters.len();
    }
    total
}
