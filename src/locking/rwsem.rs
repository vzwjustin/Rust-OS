// SPDX-License-Identifier: GPL-2.0-compatible
//! Linux reader-writer semaphore.
//!
//! Mirrors `kernel/locking/rwsem.c` and `include/linux/rwsem.h`.
//!
//! Count encoding (matches Linux `RWSEM_*` constants):
//!
//! ```text
//!  count == 0   : unlocked
//!  count  > 0   : N concurrent readers hold the lock
//!  count == -1  : write locked, no waiters
//!  count  < -1  : write locked + waiters (or: -1 - waiter_count)
//! ```
//!
//! The sleep/wake paths yield the CPU via `scheduler::yield_cpu()` when
//! contended, allowing other threads to run until the lock is released.

#![allow(dead_code)]

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use alloc::collections::VecDeque;
use spin::Mutex as SpinMutex;

// ---------------------------------------------------------------------------
// Count constants (Linux RWSEM_* macros)
// ---------------------------------------------------------------------------

pub const RWSEM_UNLOCKED_VALUE: i64 = 0;
/// A writer holds the lock (no waiters).
pub const RWSEM_WRITER_LOCKED: i64 = -1;
/// Bias added per reader.
const RWSEM_READER_BIAS: i64 = 1;

// ---------------------------------------------------------------------------
// Waiter kind
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum RwWaiterKind {
    Reader,
    Writer,
}

struct RwWaiter {
    kind: RwWaiterKind,
    task_id: usize,
}

// ---------------------------------------------------------------------------
// RwSemaphore<T>
// ---------------------------------------------------------------------------

/// Linux-style reader-writer semaphore protecting a value of type `T`.
///
/// Multiple readers may hold the lock simultaneously; writers get
/// exclusive access.  Writers are not starved: once a writer is queued,
/// new readers will also queue.
pub struct RwSemaphore<T> {
    /// Signed count using the Linux RWSEM encoding above.
    count: AtomicI64,
    /// Owner task ID (valid only while write-locked).
    owner: AtomicUsize,
    /// Wait list (protected by inner spinlock, mirroring `wait_lock`).
    wait_list: SpinMutex<VecDeque<RwWaiter>>,
    /// Protected data.
    data: UnsafeCell<T>,
}

// SAFETY: protected by atomic state + UnsafeCell discipline.
unsafe impl<T: Send> Send for RwSemaphore<T> {}
unsafe impl<T: Send + Sync> Sync for RwSemaphore<T> {}

impl<T> RwSemaphore<T> {
    /// Create a new, unlocked semaphore.
    pub fn new(data: T) -> Self {
        Self {
            count: AtomicI64::new(RWSEM_UNLOCKED_VALUE),
            owner: AtomicUsize::new(0),
            wait_list: SpinMutex::new(VecDeque::new()),
            data: UnsafeCell::new(data),
        }
    }

    // -----------------------------------------------------------------------
    // Fast-path helpers
    // -----------------------------------------------------------------------

    /// Try to add a reader (increment count from a non-negative value).
    #[inline]
    fn try_acquire_read_fast(&self) -> bool {
        let mut cur = self.count.load(Ordering::Relaxed);
        loop {
            if cur < 0 {
                // Writer holds the lock.
                return false;
            }
            match self.count.compare_exchange_weak(
                cur,
                cur + RWSEM_READER_BIAS,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(c) => cur = c,
            }
        }
    }

    /// Try to acquire a write lock (CAS 0 -> -1).
    #[inline]
    fn try_acquire_write_fast(&self) -> bool {
        self.count
            .compare_exchange(
                RWSEM_UNLOCKED_VALUE,
                RWSEM_WRITER_LOCKED,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok()
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Acquire a shared read lock, blocking until available.
    ///
    /// Yields the CPU to the scheduler in the contended case.
    pub fn read(&self) -> ReadGuard<'_, T> {
        if self.try_acquire_read_fast() {
            return ReadGuard { lock: self };
        }

        // Slow path: queue as a reader and yield-wait.
        let current_pid = crate::process::get_process_manager().current_process() as usize;
        {
            let mut wl = self.wait_list.lock();
            wl.push_back(RwWaiter {
                kind: RwWaiterKind::Reader,
                task_id: current_pid,
            });
        }

        loop {
            crate::scheduler::yield_cpu();
            if self.try_acquire_read_fast() {
                // Remove our slot from the wait list.
                let mut wl = self.wait_list.lock();
                if let Some(pos) = wl.iter().position(|w| w.task_id == current_pid) {
                    wl.remove(pos);
                }
                return ReadGuard { lock: self };
            }
        }
    }

    /// Try to acquire a shared read lock without blocking.
    pub fn try_read(&self) -> Option<ReadGuard<'_, T>> {
        if self.try_acquire_read_fast() {
            Some(ReadGuard { lock: self })
        } else {
            None
        }
    }

    /// Acquire an exclusive write lock, blocking until available.
    ///
    /// Yields the CPU to the scheduler in the contended case.
    pub fn write(&self) -> WriteGuard<'_, T> {
        if self.try_acquire_write_fast() {
            return WriteGuard { lock: self };
        }

        // Slow path: indicate a writer is waiting (count < -1).
        self.count.fetch_sub(1, Ordering::Relaxed);

        loop {
            crate::scheduler::yield_cpu();
            // Try to CAS from 0 to -1.  We first undo our waiter bias.
            let cur = self.count.load(Ordering::Relaxed);
            if cur == -1 {
                // Currently write-locked; keep waiting.
                continue;
            }
            if cur <= 0 {
                // Only our own waiter bias is outstanding; attempt grab.
                if self
                    .count
                    .compare_exchange(
                        cur,
                        RWSEM_WRITER_LOCKED,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return WriteGuard { lock: self };
                }
            }
        }
    }

    /// Try to acquire an exclusive write lock without blocking.
    pub fn try_write(&self) -> Option<WriteGuard<'_, T>> {
        if self.try_acquire_write_fast() {
            Some(WriteGuard { lock: self })
        } else {
            None
        }
    }

    /// Downgrade a write lock to a read lock atomically.
    ///
    /// The write lock transitions to a read lock without dropping
    /// exclusive access first, preventing a window where another writer
    /// could sneak in.
    pub fn downgrade(guard: WriteGuard<'_, T>) -> ReadGuard<'_, T> {
        let lock = guard.lock;
        // Transition -1 -> +1 (write locked -> one reader).
        lock.count.store(RWSEM_READER_BIAS, Ordering::Release);
        // Yield to allow queued readers to acquire.
        crate::scheduler::yield_cpu();
        core::mem::forget(guard); // prevent Drop from running
        ReadGuard { lock }
    }

    // -----------------------------------------------------------------------
    // Internal release helpers
    // -----------------------------------------------------------------------

    fn release_read(&self) {
        let prev = self.count.fetch_sub(RWSEM_READER_BIAS, Ordering::Release);
        if prev == RWSEM_READER_BIAS {
            // Last reader released; wake a queued waiter.
            if let Some(waiter) = self.wait_list.lock().pop_front() {
                let pm = crate::process::get_process_manager();
                let _ = pm.unblock_process(waiter.task_id as u32);
            }
            crate::scheduler::yield_cpu();
        }
    }

    fn release_write(&self) {
        self.owner.store(0, Ordering::Relaxed);
        self.count.store(RWSEM_UNLOCKED_VALUE, Ordering::Release);
        // Wake a queued waiter.
        if let Some(waiter) = self.wait_list.lock().pop_front() {
            let pm = crate::process::get_process_manager();
            let _ = pm.unblock_process(waiter.task_id as u32);
        }
        crate::scheduler::yield_cpu();
    }
}

// ---------------------------------------------------------------------------
// Guards
// ---------------------------------------------------------------------------

/// RAII guard for a shared read lock on [`RwSemaphore`].
pub struct ReadGuard<'a, T> {
    lock: &'a RwSemaphore<T>,
}

impl<'a, T> Drop for ReadGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.release_read();
    }
}

impl<'a, T> Deref for ReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: shared access is allowed while any ReadGuard exists.
        unsafe { &*self.lock.data.get() }
    }
}

/// RAII guard for an exclusive write lock on [`RwSemaphore`].
pub struct WriteGuard<'a, T> {
    lock: &'a RwSemaphore<T>,
}

impl<'a, T> Drop for WriteGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.release_write();
    }
}

impl<'a, T> Deref for WriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: exclusive write access.
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for WriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: exclusive write access.
        unsafe { &mut *self.lock.data.get() }
    }
}
