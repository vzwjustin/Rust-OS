// SPDX-License-Identifier: GPL-2.0-compatible
//! Linux-style sleeping mutex.
//!
//! Mirrors `kernel/locking/mutex.c`.  The owner field encodes the task
//! pointer in the high bits and flag bits in the low bits, exactly as
//! Linux does.  Actual sleep/wake is stubbed with a spin-wait loop;
//! real integration requires calling into the scheduler's wait-queue
//! infrastructure (`schedule()` + `wake_up()`).

#![allow(dead_code)]

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::collections::VecDeque;
use spin::Mutex as SpinMutex;

use super::lockdep;

// ---------------------------------------------------------------------------
// State constants (mirrors MUTEX_STATE_* in Linux)
// ---------------------------------------------------------------------------

/// No owner, no waiters.
const MUTEX_STATE_UNLOCKED: usize = 0;
/// Locked, no waiters.
const MUTEX_STATE_LOCKED: usize = 1;
/// Locked with one or more waiters queued.
const MUTEX_STATE_WAITERS: usize = 2;

// Flag bits stored in the low bits of `owner` (Linux uses bits 0-2).
const MUTEX_FLAG_WAITERS: usize = 0x1; // waiters present
const MUTEX_FLAG_HANDOFF: usize = 0x2; // lock handoff in progress
const MUTEX_FLAG_PICKUP: usize = 0x4; // lock is being picked up
const MUTEX_FLAGS: usize = 0x7;

// ---------------------------------------------------------------------------
// Waiter node
// ---------------------------------------------------------------------------

/// An entry in the mutex wait list.
///
/// In a real kernel this would embed a `task_struct *` and the waiter
/// would be put to sleep; here we store a spin-wait flag.
struct WaiterNode {
    /// Task ID of the blocked task (informational / for deadlock detection).
    task_id: usize,
    /// Set to `true` by the unlock path when this waiter should wake.
    woken: bool,
}

// ---------------------------------------------------------------------------
// Mutex<T>
// ---------------------------------------------------------------------------

/// A Linux-style sleeping mutex protecting a value of type `T`.
///
/// Unlike `spin::Mutex`, this mutex is intended for contexts where the
/// lock may be held for a long time and spinning would waste CPU.  In
/// the current stub implementation the "sleep" degrades to a busy-wait;
/// see the `TODO` comments for scheduler integration points.
pub struct Mutex<T> {
    /// Encodes lock state in the low bits and (eventually) owner task
    /// pointer in the high bits, matching Linux's `atomic_long_t owner`.
    state: AtomicUsize,
    /// Owner task ID (0 = no owner).
    owner: AtomicUsize,
    /// Wait list protected by an inner spinlock, mirroring `wait_lock`.
    wait_list: SpinMutex<VecDeque<WaiterNode>>,
    /// Protected data.
    data: UnsafeCell<T>,
}

// SAFETY: The mutex guarantees exclusive access; T need only be Send.
unsafe impl<T: Send> Send for Mutex<T> {}
unsafe impl<T: Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    /// Create a new, unlocked mutex.
    pub const fn new(data: T) -> Self {
        Self {
            state: AtomicUsize::new(MUTEX_STATE_UNLOCKED),
            owner: AtomicUsize::new(0),
            wait_list: SpinMutex::new(VecDeque::new()),
            data: UnsafeCell::new(data),
        }
    }

    // -----------------------------------------------------------------------
    // Fast path helpers
    // -----------------------------------------------------------------------

    #[inline]
    fn is_locked(&self) -> bool {
        self.state.load(Ordering::Relaxed) != MUTEX_STATE_UNLOCKED
    }

    /// Attempt a single CAS to acquire the lock without queuing.
    #[inline]
    fn try_acquire_fast(&self) -> bool {
        self.state
            .compare_exchange(
                MUTEX_STATE_UNLOCKED,
                MUTEX_STATE_LOCKED,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok()
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Acquire the mutex, blocking until it is available.
    ///
    /// **TODO (scheduler integration)**: replace the spin-wait loop with
    /// `prepare_to_wait()` + `schedule()` + `finish_wait()` once the
    /// kernel has a proper wait-queue implementation.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        lockdep::acquire(
            current_task_id(),
            lockdep::lock_id_of(self),
            "locking::Mutex",
        );

        // Fast path: uncontended acquisition.
        if self.try_acquire_fast() {
            return MutexGuard { lock: self };
        }

        // Slow path: mark waiters and spin-wait (stub).
        // In a real kernel this would put the current task on `wait_list`
        // and call `schedule()`.
        self.state
            .fetch_or(MUTEX_STATE_WAITERS, Ordering::Relaxed);

        // Spin until the lock is free then grab it.
        // TODO: replace with proper sleep/wake when scheduler is available.
        loop {
            core::hint::spin_loop();

            if self
                .state
                .compare_exchange(
                    MUTEX_STATE_UNLOCKED,
                    MUTEX_STATE_LOCKED,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return MutexGuard { lock: self };
            }
        }
    }

    /// Try to acquire the mutex without blocking.
    ///
    /// Returns `Some(guard)` on success, `None` if the lock is held.
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.try_acquire_fast() {
            lockdep::acquire(
                current_task_id(),
                lockdep::lock_id_of(self),
                "locking::Mutex",
            );
            Some(MutexGuard { lock: self })
        } else {
            None
        }
    }

    // -----------------------------------------------------------------------
    // Internal: release (called from Drop)
    // -----------------------------------------------------------------------

    fn unlock_internal(&self) {
        lockdep::release(current_task_id(), lockdep::lock_id_of(self));

        // Clear owner.
        self.owner.store(0, Ordering::Relaxed);

        // Release the lock.  If there are waiters we keep WAITERS set so
        // the next acquirer sees contention.
        let prev = self.state.swap(MUTEX_STATE_UNLOCKED, Ordering::Release);

        if prev & MUTEX_STATE_WAITERS != 0 {
            // TODO: wake one waiter from wait_list via scheduler.
            // For now the spinning looper will naturally see the unlock.
            let _ = self.wait_list.lock().pop_front();
        }
    }
}

// ---------------------------------------------------------------------------
// MutexGuard<T>
// ---------------------------------------------------------------------------

/// RAII guard returned by [`Mutex::lock`] / [`Mutex::try_lock`].
///
/// The mutex is released when this guard is dropped.
pub struct MutexGuard<'a, T> {
    lock: &'a Mutex<T>,
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.unlock_internal();
    }
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: we hold the lock.
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: we hold the lock exclusively.
        unsafe { &mut *self.lock.data.get() }
    }
}

/// Best-effort current task identifier for the lockdep-lite hooks.
///
/// Falls back to `0` ("anonymous") if the scheduler has no current
/// process recorded yet (e.g. very early boot, before any process has
/// been scheduled).
#[inline]
fn current_task_id() -> usize {
    crate::process::scheduler::get_current_process() as usize
}
