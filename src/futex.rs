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
use core::sync::atomic::{AtomicI32, AtomicU64, Ordering};
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

// ── Priority-Inheritance futex state ────────────────────────────────────

/// Priority-inheritance state for a PI-futex at a given virtual address.
struct PiState {
    /// PID of the task currently holding the PI-futex.
    owner_pid: u32,
    /// Owner's original priority (as u8 discriminant) before any inheritance boost.
    saved_priority: u8,
    /// Pids (and their original priority as u8) waiting to acquire this PI-futex.
    waiters: alloc::collections::VecDeque<(u32, u8)>,
}

static PI_STATES: spin::Mutex<alloc::collections::BTreeMap<usize, PiState>> =
    spin::Mutex::new(alloc::collections::BTreeMap::new());

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

/// FUTEX_WAIT_REQUEUE_PI — wait on a non-PI futex; wake via CMP_REQUEUE_PI
/// to a PI futex. The waiter blocks on `uaddr` and is requeued to `uaddr2`
/// by the waker (FUTEX_CMP_REQUEUE_PI). On requeue, we try to acquire `uaddr2`.
pub fn futex_wait_requeue_pi(
    uaddr:   *mut i32,
    val:     i32,
    uaddr2:  *mut i32,
    timeout: Option<&FutexTimeout>,
) -> i32 {
    if uaddr.is_null() || uaddr2.is_null() {
        return -14; // EFAULT
    }

    // Block on uaddr — inlined from FUTEX_WAIT but WITHOUT the post-wakeup
    // *uaddr == val check. futex_cmp_requeue_pi intentionally changes *uaddr
    // before waking us; the check in the regular futex_wait path would fire
    // and return -EAGAIN, preventing us from ever reaching the PI-futex CAS.
    {
        let current = unsafe { core::ptr::read_volatile(uaddr) };
        if current != val {
            return -11; // EAGAIN
        }

        let key1 = uaddr as usize;
        let hash1_w = futex_hash(key1);
        let our_pid_w = crate::process::current_pid();
        let our_tid_w = crate::process::thread::get_thread_manager().current_thread();

        {
            let bucket = &FUTEX_BUCKETS[hash1_w];
            let mut b = bucket.lock();
            b.waiters.push((key1, FutexWaiter {
                pid: our_pid_w,
                tid: our_tid_w,
                bitset: FUTEX_BITSET_MATCH_ANY,
            }));
        }

        if let Some(to) = timeout {
            if to.expired() {
                let bucket = &FUTEX_BUCKETS[hash1_w];
                let mut b = bucket.lock();
                b.waiters.retain(|(k, w)| {
                    !(*k == key1 && w.pid == our_pid_w && w.tid == our_tid_w)
                });
                return -110; // ETIMEDOUT
            }
        }

        let pm = crate::process::get_process_manager();
        let _ = pm.block_process(our_pid_w);
        // After wakeup: do NOT re-check *uaddr. The condvar value was changed
        // intentionally by CMP_REQUEUE_PI — fall through to the PI-futex CAS.
    }

    // Woken. Now try to acquire the PI futex at uaddr2.
    let pi_key  = uaddr2 as usize;
    let our_pid = crate::process::current_pid();
    let our_tid = crate::process::thread::get_thread_manager().current_thread() as i32;

    // Atomically CAS *uaddr2 from 0 to our_tid
    let pi_atomic = unsafe { &*(uaddr2 as *const AtomicI32) };
    let prev = pi_atomic.compare_exchange(0, our_tid, Ordering::AcqRel, Ordering::Acquire)
        .unwrap_or_else(|v| v);

    if prev == 0 {
        // Acquired the PI-futex immediately
        let mut pi = PI_STATES.lock();
        let state = pi.entry(pi_key).or_insert_with(|| PiState {
            owner_pid:      our_pid,
            saved_priority: crate::process::scheduler::get_process_priority(our_pid)
                                .map(|p| p as u8).unwrap_or(2u8),
            waiters:        alloc::collections::VecDeque::new(),
        });
        state.owner_pid = our_pid;
        state.saved_priority = crate::process::scheduler::get_process_priority(our_pid)
            .map(|p| p as u8).unwrap_or(2u8);
        return 0;
    }

    // PI-futex is owned — register as waiter and boost owner
    {
        let mut pi = PI_STATES.lock();
        let state = pi.entry(pi_key).or_insert_with(|| PiState {
            owner_pid:      prev as u32,
            saved_priority: crate::process::scheduler::get_process_priority(prev as u32)
                                .map(|p| p as u8).unwrap_or(2u8),
            waiters:        alloc::collections::VecDeque::new(),
        });
        let our_prio = crate::process::scheduler::get_process_priority(our_pid)
            .map(|p| p as u8).unwrap_or(2u8);
        state.waiters.push_back((our_pid, our_prio));
        // Boost owner to at least our priority (RealTime = 0 = highest)
        let _ = crate::process::scheduler::set_process_priority(
            state.owner_pid,
            crate::process::Priority::High,
        );
    }

    // Block until the owner calls futex_pi_unlock
    let pm = crate::process::get_process_manager();
    let _ = pm.block_process(our_pid);

    0
}

/// FUTEX_CMP_REQUEUE_PI — conditional requeue from non-PI to PI-futex.
/// Wakes `val` waiters on `uaddr` directly, then requeues up to `val2`
/// more to the PI-futex at `uaddr2`. Returns woken + requeued.
pub fn futex_cmp_requeue_pi(
    uaddr:   *mut i32,
    uaddr2:  *mut i32,
    val:     i32,
    val2:    i32,
    cmpval:  i32,
) -> i32 {
    if uaddr.is_null() || uaddr2.is_null() {
        return -14; // EFAULT
    }

    // Atomic check: *uaddr must equal cmpval
    let current = unsafe { &*(uaddr as *const AtomicI32) }.load(Ordering::Acquire);
    if current != cmpval {
        return -11; // EAGAIN
    }

    let key1 = uaddr  as usize;
    let key2 = uaddr2 as usize;
    let hash1 = futex_hash(key1);
    let hash2 = futex_hash(key2);

    let mut woken    = 0i32;
    let mut requeued = 0i32;

    // Wake up to `val` waiters directly from uaddr
    {
        let bucket = &FUTEX_BUCKETS[hash1];
        let mut b = bucket.lock();
        let mut to_wake = alloc::vec![];
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

    // Requeue up to `val2` more waiters from uaddr to uaddr2 (PI)
    if val2 > 0 && hash1 != hash2 {
        // Acquire bucket locks in index order to prevent ABBA deadlock when a
        // concurrent cmp_requeue_pi maps to the same two buckets in reverse.
        let (lo_idx, hi_idx) = if hash1 < hash2 {
            (hash1, hash2)
        } else {
            (hash2, hash1)
        };
        let mut guard_lo = FUTEX_BUCKETS[lo_idx].lock();
        let mut guard_hi = FUTEX_BUCKETS[hi_idx].lock();
        // Rebind to the original b1/b2 names the rest of the block expects.
        let (b1, b2): (&mut FutexBucket, &mut FutexBucket) = if hash1 < hash2 {
            (&mut *guard_lo, &mut *guard_hi)
        } else {
            (&mut *guard_hi, &mut *guard_lo)
        };
        let mut to_requeue = alloc::vec![];
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
            let waiter_pid = waiter.pid;
            b2.waiters.push((key2, waiter));
            // Register waiter in PI_STATES and boost owner
            let mut pi = PI_STATES.lock();
            let state = pi.entry(key2).or_insert_with(|| PiState {
                owner_pid:      0,
                saved_priority: 2u8, // Priority::Normal as u8
                waiters:        alloc::collections::VecDeque::new(),
            });
            let waiter_prio = crate::process::scheduler::get_process_priority(waiter_pid)
                .map(|p| p as u8).unwrap_or(2u8);
            state.waiters.push_back((waiter_pid, waiter_prio));
            if state.owner_pid != 0 {
                let _ = crate::process::scheduler::set_process_priority(
                    state.owner_pid,
                    crate::process::Priority::High,
                );
            }
            requeued += 1;
        }
    } else if val2 > 0 {
        // Same hash bucket — just re-key
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

/// Release a PI-futex. Restores the owner's priority and wakes the
/// highest-priority waiter (if any), transferring the lock to it.
pub fn futex_pi_unlock(uaddr: *mut i32) -> i32 {
    if uaddr.is_null() {
        return -14; // EFAULT
    }

    let pi_key  = uaddr as usize;
    let our_pid = crate::process::current_pid();

    let next_pid = {
        let mut pi = PI_STATES.lock();
        if let Some(state) = pi.get_mut(&pi_key) {
            if state.owner_pid != our_pid {
                return -1; // EPERM
            }
            // Restore priority: convert saved u8 back to Priority discriminant
            let restored_priority = match state.saved_priority {
                0 => crate::process::Priority::RealTime,
                1 => crate::process::Priority::High,
                3 => crate::process::Priority::Low,
                4 => crate::process::Priority::Idle,
                _ => crate::process::Priority::Normal,
            };
            let _ = crate::process::scheduler::set_process_priority(our_pid, restored_priority);
            state.waiters.pop_front()
        } else {
            None
        }
    };

    if let Some((next, _waiter_prio)) = next_pid {
        // Clear the futex so the new owner can CAS 0 → own_tid (standard FUTEX_UNLOCK_PI).
        unsafe {
            core::ptr::write_volatile(uaddr, 0);
        }
        // Remove the PI_STATES entry; the woken waiter will recreate it on CAS success.
        PI_STATES.lock().remove(&pi_key);
        // Wake the new owner — it will re-establish PI_STATES ownership.
        let pm = crate::process::get_process_manager();
        let _ = pm.unblock_process(next);
    } else {
        // No waiters — clear the futex and remove state.
        unsafe {
            core::ptr::write_volatile(uaddr, 0);
        }
        PI_STATES.lock().remove(&pi_key);
    }

    0
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
        FUTEX_WAIT_REQUEUE_PI => futex_wait_requeue_pi(uaddr, val, uaddr2, timeout),
        FUTEX_CMP_REQUEUE_PI => futex_cmp_requeue_pi(uaddr, uaddr2, val, val2, val3),
        FUTEX_UNLOCK_PI => futex_pi_unlock(uaddr),
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
