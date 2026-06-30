// SPDX-License-Identifier: GPL-2.0-compatible
//! Linux counting semaphore.
//!
//! Mirrors `kernel/locking/semaphore.c` and `include/linux/semaphore.h`.
//!
//! The `count` field represents how many more tasks can acquire the
//! semaphore before blocking, exactly as Linux's `struct semaphore`.
//!
//! The `down_interruptible` and `down_timeout` variants return error
//! codes matching Linux errno values (-EINTR, -ETIME) so callers can
//! be ported mechanically.
//!
//! **Scheduler integration note**: `down()` and friends spin-wait in
//! the stub.  Replace the `spin_loop()` paths with proper
//! `prepare_to_wait()` / `schedule()` / `finish_wait()` calls.

#![allow(dead_code)]

use core::sync::atomic::{AtomicI32, Ordering};

use alloc::collections::VecDeque;
use spin::Mutex as SpinMutex;

// ---------------------------------------------------------------------------
// Error codes (Linux errno subset)
// ---------------------------------------------------------------------------

/// Interrupted by a signal (`-EINTR`).
pub const EINTR: i32 = -4;
/// Operation timed out (`-ETIME`).
pub const ETIME: i32 = -62;

// ---------------------------------------------------------------------------
// Waiter node
// ---------------------------------------------------------------------------

struct SemWaiter {
    task_id: usize,
    /// Set by `up()` when this waiter is chosen to wake.
    woken: bool,
}

// ---------------------------------------------------------------------------
// Semaphore
// ---------------------------------------------------------------------------

/// Linux counting semaphore.
///
/// Initialise with `Semaphore::new(n)` where `n` is the initial permit
/// count.  A binary semaphore is `Semaphore::new(1)`.
pub struct Semaphore {
    /// Remaining permits.  Negative values encode the number of blocked
    /// waiters (mirrors Linux's representation after Linux 4.x cleanup).
    count: AtomicI32,
    /// Serialises access to `wait_list` (mirrors `sem->lock`).
    wait_list: SpinMutex<VecDeque<SemWaiter>>,
}

// SAFETY: atomics + inner spin lock make this safe to share.
unsafe impl Send for Semaphore {}
unsafe impl Sync for Semaphore {}

impl Semaphore {
    /// Create a new semaphore with `val` initial permits.
    pub fn new(val: i32) -> Self {
        Self {
            count: AtomicI32::new(val),
            wait_list: SpinMutex::new(VecDeque::new()),
        }
    }

    // -----------------------------------------------------------------------
    // P (decrement / acquire) operations
    // -----------------------------------------------------------------------

    /// Decrement the semaphore, blocking until a permit is available.
    ///
    /// Mirrors `down()`.  **TODO**: replace spin-wait with
    /// `schedule()` when the scheduler supports it.
    pub fn down(&self) {
        // Fast path.
        if self
            .count
            .fetch_update(Ordering::Acquire, Ordering::Relaxed, |c| {
                if c > 0 { Some(c - 1) } else { None }
            })
            .is_ok()
        {
            return;
        }

        // Slow path: queue and spin-wait.
        {
            let mut wl = self.wait_list.lock();
            wl.push_back(SemWaiter { task_id: 0, woken: false });
        }

        // TODO: call schedule() here instead of spinning.
        loop {
            core::hint::spin_loop();
            if self
                .count
                .fetch_update(Ordering::Acquire, Ordering::Relaxed, |c| {
                    if c > 0 { Some(c - 1) } else { None }
                })
                .is_ok()
            {
                // Remove our wait entry.
                let mut wl = self.wait_list.lock();
                wl.pop_front();
                return;
            }
        }
    }

    /// Decrement the semaphore if a permit is immediately available.
    ///
    /// Returns `true` if the semaphore was acquired, `false` otherwise.
    /// Mirrors `down_trylock()`.  Safe to call from interrupt context.
    pub fn down_trylock(&self) -> bool {
        self.count
            .fetch_update(Ordering::Acquire, Ordering::Relaxed, |c| {
                if c > 0 { Some(c - 1) } else { None }
            })
            .is_ok()
    }

    /// Decrement the semaphore, returning `-EINTR` if interrupted.
    ///
    /// Mirrors `down_interruptible()`.
    ///
    /// **TODO**: hook into signal-pending check once signal delivery is
    /// implemented.  Currently behaves identically to [`down`](Self::down)
    /// and always returns `0`.
    pub fn down_interruptible(&self) -> i32 {
        // TODO: check signal_pending(current) and return EINTR.
        self.down();
        0
    }

    /// Decrement the semaphore with a timeout given in jiffies.
    ///
    /// Returns `0` on success, [`ETIME`] if the timeout expires.
    /// Mirrors `down_timeout()`.
    ///
    /// **TODO**: replace spin-count estimate with real timer infrastructure.
    pub fn down_timeout(&self, timeout_jiffies: u64) -> i32 {
        // Rough spin budget: 1 jiffy ≈ 1_000_000 spin iterations (placeholder).
        let mut budget = timeout_jiffies.saturating_mul(1_000_000);

        loop {
            if self
                .count
                .fetch_update(Ordering::Acquire, Ordering::Relaxed, |c| {
                    if c > 0 { Some(c - 1) } else { None }
                })
                .is_ok()
            {
                return 0;
            }

            if budget == 0 {
                return ETIME;
            }
            budget -= 1;
            core::hint::spin_loop();
        }
    }

    // -----------------------------------------------------------------------
    // V (increment / release) operation
    // -----------------------------------------------------------------------

    /// Increment the semaphore, waking one blocked waiter if any.
    ///
    /// Mirrors `up()`.  Safe to call from interrupt context.
    pub fn up(&self) {
        // If there are waiters, wake the first one instead of incrementing.
        let mut wl = self.wait_list.lock();
        if let Some(waiter) = wl.front_mut() {
            waiter.woken = true;
            // TODO: wake_up_process(waiter.task) via scheduler.
            // The spinning loop in down() will see count > 0 after we drop
            // the wait_list lock, but we need the count increment so it
            // can actually take the permit.
            self.count.fetch_add(1, Ordering::Release);
            return;
        }
        drop(wl);
        self.count.fetch_add(1, Ordering::Release);
    }

    // -----------------------------------------------------------------------
    // Introspection helpers
    // -----------------------------------------------------------------------

    /// Return the current permit count (may race with concurrent ops).
    pub fn count(&self) -> i32 {
        self.count.load(Ordering::Relaxed)
    }

    /// Return `true` if no permits are available.
    pub fn is_locked(&self) -> bool {
        self.count.load(Ordering::Relaxed) <= 0
    }
}
