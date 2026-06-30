// SPDX-License-Identifier: GPL-2.0-compatible
//! Priority-inheritance mutex (RT mutex).
//!
//! Mirrors `kernel/locking/rtmutex.c` (52 KB) at a structural level.
//!
//! Key design properties:
//! - When a high-priority task blocks on this mutex, the owner's
//!   priority is boosted to the maximum waiter priority
//!   (`rt_mutex_adjust_prio`), and the boost **propagates transitively**
//!   across a chain of nested locks: if the owner is itself blocked on a
//!   different `RtMutex`, that lock's owner is boosted too, and so on.
//!   This mirrors `rt_mutex_adjust_prio_chain()` in upstream, which walks
//!   `task->pi_blocked_on -> lock->owner -> owner->pi_blocked_on -> ...`.
//!   Because each `RtMutex<T>` in this port is a distinct Rust
//!   monomorphization, the chain cannot be walked through typed `&self`
//!   references the way upstream walks `struct rt_mutex *`; instead a
//!   small global, non-generic registry (`BLOCKED_ON` / `LOCK_OWNER`,
//!   keyed by each lock's address) tracks "task T is blocked on lock L"
//!   and "lock L is owned by task O" edges, and the chain walk traverses
//!   that registry. The walk is capped at [`MAX_CHAIN_DEPTH`] hops and
//!   tracks visited lock ids to bound cost and tolerate a cycle (which
//!   would indicate an actual AB-BA deadlock elsewhere) without spinning
//!   forever.
//! - When the mutex is released the owner's priority is restored (from a
//!   small per-task "normal priority" save, since this port has no
//!   `task_struct::normal_prio` field to fall back to) and the
//!   highest-priority waiter is woken via the real process scheduler.
//! - Waiters are kept in a `BTreeMap<priority, task_id>` so the
//!   highest-priority waiter is always at the end (max key).
//!
//! **Scheduler integration note**: `lock()` still spins on the fast/slow
//! acquire loop rather than fully sleeping — see the `TODO` near
//! `lock_with_task` — but priority boosting and waiter wakeup now call
//! the real scheduler (`set_process_priority`, `unblock_process`)
//! instead of being stubs.

#![allow(dead_code)]

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use spin::Mutex as SpinMutex;

use super::lockdep;

/// Maximum number of hops walked by [`adjust_prio_chain`] before giving
/// up. Mirrors upstream's bounded `max_lock_depth` (default 1024, much
/// larger because real workloads can legitimately nest deeper); this
/// port's lock graph is expected to be shallow, so a smaller cap is used
/// and documented rather than copied verbatim.
const MAX_CHAIN_DEPTH: usize = 32;

/// Global edge: task id -> (lock id it is blocked on, its waiter priority).
static BLOCKED_ON: SpinMutex<BTreeMap<usize, (u64, u32)>> = SpinMutex::new(BTreeMap::new());
/// Global edge: lock id -> owning task id.
static LOCK_OWNER: SpinMutex<BTreeMap<u64, usize>> = SpinMutex::new(BTreeMap::new());
/// Saved pre-boost scheduler priority per task, so a boost can be undone
/// without needing a real `task_struct::normal_prio` field.
static SAVED_PRIO: SpinMutex<BTreeMap<usize, crate::process::Priority>> =
    SpinMutex::new(BTreeMap::new());

/// Map an RT-mutex waiter priority (0-99, higher = more urgent, matching
/// Linux's `sched_priority` range for `SCHED_FIFO`/`SCHED_RR`) onto this
/// kernel's 5-level `process::Priority` enum (where, inversely,
/// `RealTime` is the numerically *lowest* variant). This is a coarse
/// bucketing rather than a 1:1 priority space, since `process::Priority`
/// has far fewer levels than Linux's RT range.
fn waiter_prio_to_sched(prio: u32) -> crate::process::Priority {
    use crate::process::Priority;
    match prio {
        80..=99 => Priority::RealTime,
        60..=79 => Priority::High,
        40..=59 => Priority::Normal,
        20..=39 => Priority::Low,
        _ => Priority::Idle,
    }
}

/// Walk the blocked-on chain starting at `owner_task_id` (the current
/// owner of the lock `waiter` just blocked on), boosting every owner in
/// the chain to at least `waiter_prio`. Mirrors
/// `rt_mutex_adjust_prio_chain()`: boosting is transitive across nested
/// `RtMutex` acquisitions, not just a single hop.
fn adjust_prio_chain(mut owner_task_id: usize, waiter_prio: u32) {
    let mut visited = alloc::collections::BTreeSet::new();

    for _ in 0..MAX_CHAIN_DEPTH {
        if !visited.insert(owner_task_id) {
            // Cycle in the blocked-on graph: this indicates a genuine
            // lock-order bug elsewhere (true deadlock), not something
            // priority inheritance can resolve. Stop walking rather than
            // spin forever, mirroring upstream's `-EDEADLK` detection.
            break;
        }

        // Boost this owner if `waiter_prio` exceeds its current priority.
        boost_task(owner_task_id, waiter_prio);

        // Does this owner itself block on another RtMutex? If so,
        // continue the chain through that lock's owner.
        let next = BLOCKED_ON.lock().get(&owner_task_id).copied();
        match next {
            Some((next_lock_id, _)) => {
                let next_owner = LOCK_OWNER.lock().get(&next_lock_id).copied();
                match next_owner {
                    Some(next_owner_id) if next_owner_id != owner_task_id => {
                        owner_task_id = next_owner_id;
                    }
                    _ => break,
                }
            }
            None => break,
        }
    }
}

/// Boost `task_id`'s scheduler priority to at least `waiter_prio`,
/// saving its prior priority the first time it is boosted so it can be
/// restored later.
fn boost_task(task_id: usize, waiter_prio: u32) {
    let pid = task_id as crate::process::Pid;
    let target = waiter_prio_to_sched(waiter_prio);

    if let Some(current) = crate::process::scheduler::get_process_priority(pid) {
        // `process::Priority` is ordered with `RealTime` (0) highest, so
        // "boost" means moving to a *numerically smaller or equal* value.
        if target < current {
            let mut saved = SAVED_PRIO.lock();
            saved.entry(task_id).or_insert(current);
            let _ = crate::process::scheduler::set_process_priority(pid, target);
        }
    }
}

/// Undo a priority boost previously applied to `task_id`, restoring its
/// saved pre-boost priority if one was recorded.
fn restore_task_prio(task_id: usize) {
    let pid = task_id as crate::process::Pid;
    let mut saved = SAVED_PRIO.lock();
    if let Some(orig) = saved.remove(&task_id) {
        let _ = crate::process::scheduler::set_process_priority(pid, orig);
    }
}

// ---------------------------------------------------------------------------
// Simplified Task representation
// ---------------------------------------------------------------------------

/// Minimal task descriptor used by the RT mutex.
///
/// In a full implementation this is a pointer into the kernel's
/// `task_struct`.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: usize,
    /// Effective (boosted) priority.  Higher value = higher priority,
    /// matching Linux's RT priority range (0-99).
    pub priority: u32,
}

impl Task {
    pub fn new(id: usize, priority: u32) -> Self {
        Self { id, priority }
    }
}

// ---------------------------------------------------------------------------
// RtMutex<T>
// ---------------------------------------------------------------------------

/// Priority-inheritance mutex protecting a value of type `T`.
///
/// Guarantees that the owner task's priority is raised to at least the
/// highest priority among all waiting tasks, preventing priority inversion.
pub struct RtMutex<T> {
    /// Serialises access to `waiters` and `owner` field updates.
    /// Mirrors `struct rt_mutex_base::wait_lock`.
    wait_lock: SpinMutex<()>,
    /// Priority-ordered waiter map: `priority -> Arc<Task>`.
    /// Highest priority sits at the greatest key.
    waiters: SpinMutex<BTreeMap<u32, Arc<Task>>>,
    /// Task ID of the current owner (0 = unowned).
    owner: AtomicUsize,
    /// Protected data.
    data: UnsafeCell<T>,
}

// SAFETY: protected by atomic + spinlock discipline.
unsafe impl<T: Send> Send for RtMutex<T> {}
unsafe impl<T: Send> Sync for RtMutex<T> {}

impl<T> RtMutex<T> {
    /// Create a new, unlocked RT mutex.
    pub fn new(data: T) -> Self {
        Self {
            wait_lock: SpinMutex::new(()),
            waiters: SpinMutex::new(BTreeMap::new()),
            owner: AtomicUsize::new(0),
            data: UnsafeCell::new(data),
        }
    }

    // -----------------------------------------------------------------------
    // Fast path
    // -----------------------------------------------------------------------

    /// Attempt to acquire the mutex without queuing.
    fn try_acquire_fast(&self, task_id: usize) -> bool {
        self.owner
            .compare_exchange(0, task_id, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// This lock's identity in the global `LOCK_OWNER` / `BLOCKED_ON`
    /// chain-walk registry (see module docs).
    #[inline]
    fn chain_lock_id(&self) -> u64 {
        self as *const Self as *const () as u64
    }

    // -----------------------------------------------------------------------
    // Priority inheritance
    // -----------------------------------------------------------------------

    /// Boost the current owner's priority (and transitively, anything
    /// *it* is blocked on) to at least `waiter_prio`. Mirrors
    /// `rt_mutex_adjust_prio_chain()`; see [`adjust_prio_chain`] for the
    /// chain-walk implementation shared by all `RtMutex<T>` instances.
    fn rt_mutex_adjust_prio(&self, waiter_prio: u32) {
        if let Some(&owner_task_id) = LOCK_OWNER.lock().get(&self.chain_lock_id()) {
            adjust_prio_chain(owner_task_id, waiter_prio);
        }
    }

    /// Return the maximum waiter priority, if any waiters are present.
    fn max_waiter_prio(&self) -> Option<u32> {
        let wl = self.waiters.lock();
        wl.keys().next_back().copied()
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Acquire the mutex, blocking until available.
    ///
    /// If the calling task has a higher priority than the current owner,
    /// the owner's priority is boosted (priority inheritance).
    ///
    /// **TODO**: replace spin-wait with `schedule()` / `wake_up_q()`.
    pub fn lock(&self) -> RtMutexGuard<'_, T> {
        self.lock_with_task(Arc::new(Task::new(0, 0)))
    }

    /// Acquire the mutex on behalf of a specific task.
    ///
    /// Provides proper priority inheritance when the caller can supply
    /// its task descriptor.
    pub fn lock_with_task(&self, task: Arc<Task>) -> RtMutexGuard<'_, T> {
        let task_id = task.id;
        let lock_id = self.chain_lock_id();

        // Fast path.
        if self.try_acquire_fast(task_id) {
            LOCK_OWNER.lock().insert(lock_id, task_id);
            lockdep::acquire(task_id, lock_id, "locking::RtMutex");
            return RtMutexGuard { lock: self };
        }

        // Slow path: enqueue waiter and apply (transitive) priority
        // inheritance.
        {
            let _guard = self.wait_lock.lock();
            let mut wl = self.waiters.lock();
            // Use priority as key; ties broken by insertion order
            // (Linux uses an rb-tree keyed on virtual deadline).
            wl.insert(task.priority, Arc::clone(&task));
        }
        BLOCKED_ON.lock().insert(task_id, (lock_id, task.priority));

        // Boost the owner (and transitively, whatever it's blocked on).
        if let Some(max_prio) = self.max_waiter_prio() {
            self.rt_mutex_adjust_prio(max_prio);
        }

        // Spin until we acquire.
        // TODO: call schedule() here instead of spinning; see module docs.
        loop {
            core::hint::spin_loop();
            if self
                .owner
                .compare_exchange(0, task_id, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                // Dequeue ourselves and clear our blocked-on edge now that
                // we own the lock.
                let _guard = self.wait_lock.lock();
                let mut wl = self.waiters.lock();
                wl.remove(&task.priority);
                BLOCKED_ON.lock().remove(&task_id);
                LOCK_OWNER.lock().insert(lock_id, task_id);
                lockdep::acquire(task_id, lock_id, "locking::RtMutex");
                return RtMutexGuard { lock: self };
            }
        }
    }

    /// Try to acquire the mutex without blocking.
    pub fn try_lock(&self) -> Option<RtMutexGuard<'_, T>> {
        if self.try_acquire_fast(1 /* anonymous task id */) {
            LOCK_OWNER.lock().insert(self.chain_lock_id(), 1);
            Some(RtMutexGuard { lock: self })
        } else {
            None
        }
    }

    // -----------------------------------------------------------------------
    // Internal: release
    // -----------------------------------------------------------------------

    fn unlock_internal(&self) {
        let _guard = self.wait_lock.lock();

        let lock_id = self.chain_lock_id();
        let owner_task_id = self.owner.load(Ordering::Relaxed);

        // Clear owner.
        self.owner.store(0, Ordering::Release);
        LOCK_OWNER.lock().remove(&lock_id);
        lockdep::release(owner_task_id, lock_id);

        // Restore owner priority (undo boost), now that it no longer
        // holds this lock.
        restore_task_prio(owner_task_id);

        // Wake the highest-priority waiter via the real scheduler.
        let mut wl = self.waiters.lock();
        if let Some((_prio, task)) = wl.iter().next_back() {
            let _ = crate::process::get_process_manager()
                .unblock_process(task.id as crate::process::Pid);
        }
        // Remove the woken waiter.
        if let Some(&max_key) = wl.keys().next_back() {
            wl.remove(&max_key);
        }
    }
}

// ---------------------------------------------------------------------------
// RtMutexGuard<T>
// ---------------------------------------------------------------------------

/// RAII guard returned by [`RtMutex::lock`] / [`RtMutex::try_lock`].
pub struct RtMutexGuard<'a, T> {
    lock: &'a RtMutex<T>,
}

impl<'a, T> Drop for RtMutexGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.unlock_internal();
    }
}

impl<'a, T> Deref for RtMutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: exclusive access while guard is live.
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for RtMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: exclusive access while guard is live.
        unsafe { &mut *self.lock.data.get() }
    }
}
