// SPDX-License-Identifier: GPL-2.0-compatible
//! Linux completion variable.
//!
//! Mirrors `include/linux/completion.h` and `kernel/sched/completion.c`.
//!
//! A completion is a one-shot (or reusable) event flag.  A task calls
//! `wait()` and blocks until another task (or interrupt handler) calls
//! `complete()` or `complete_all()`.
//!
//! The `done` counter mirrors Linux's `struct completion::done` field:
//! - `done == 0`: not yet completed; waiters will block.
//! - `done > 0`: completed; each `wait()` decrements by 1.
//! - `done == u32::MAX`: completed "all" – waiters don't decrement
//!   (mirrors `COMPLETION_INITIALIZER_ONSTACK` / `complete_all()`).
//!
//! **Scheduler integration note**: `wait()` spins in the stub.
//! Replace with `add_wait_queue()` + `schedule()` + `remove_wait_queue()`.

#![allow(dead_code)]

use core::sync::atomic::{AtomicU32, Ordering};

use alloc::collections::VecDeque;
use spin::Mutex as SpinMutex;

// ---------------------------------------------------------------------------
// Constants (mirrors Linux COMPLETION_* macros)
// ---------------------------------------------------------------------------

/// Sentinel value set by `complete_all()`: waiters never decrement.
const COMPLETION_ALL_DONE: u32 = u32::MAX;

// ---------------------------------------------------------------------------
// Waiter node
// ---------------------------------------------------------------------------

struct CompWaiter {
    task_id: usize,
    /// Set by `complete()` / `complete_all()` when this waiter is woken.
    woken: bool,
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

/// Linux completion variable.
///
/// Create with [`Completion::new`], signal with [`complete`](Self::complete)
/// or [`complete_all`](Self::complete_all), and wait with
/// [`wait`](Self::wait).
pub struct Completion {
    /// Completed count (0 = not done).
    done: AtomicU32,
    /// Wait queue (inner spinlock mirrors `wait_lock`).
    wait: SpinMutex<VecDeque<CompWaiter>>,
}

// SAFETY: atomics + inner spin lock make this safe to share.
unsafe impl Send for Completion {}
unsafe impl Sync for Completion {}

impl Completion {
    /// Create a new, unsignalled completion.
    pub fn new() -> Self {
        Self {
            done: AtomicU32::new(0),
            wait: SpinMutex::new(VecDeque::new()),
        }
    }

    // -----------------------------------------------------------------------
    // Signal side
    // -----------------------------------------------------------------------

    /// Signal one waiter.
    ///
    /// Increments `done` by 1 and wakes the oldest queued task.
    /// Mirrors `complete()`.
    pub fn complete(&self) {
        let mut wq = self.wait.lock();

        // Increment done (cap at COMPLETION_ALL_DONE - 1 to avoid wrap).
        self.done
            .fetch_update(Ordering::Release, Ordering::Relaxed, |d| {
                if d == COMPLETION_ALL_DONE {
                    Some(d) // already complete_all'd; leave sentinel
                } else {
                    Some(d.saturating_add(1))
                }
            })
            .ok();

        // Wake one waiter.
        if let Some(w) = wq.front_mut() {
            w.woken = true;
            // TODO: wake_up_process(w.task_id) via scheduler.
        }
        // Remove the woken waiter (or it will be cleaned up in wait()).
        wq.pop_front();
    }

    /// Signal all waiters.
    ///
    /// Sets `done` to `u32::MAX` so that all current and future
    /// `wait()` calls return immediately without decrementing `done`.
    /// Mirrors `complete_all()`.
    pub fn complete_all(&self) {
        let mut wq = self.wait.lock();
        self.done.store(COMPLETION_ALL_DONE, Ordering::Release);
        // TODO: wake all tasks in wq via scheduler.
        for w in wq.iter_mut() {
            w.woken = true;
        }
        wq.clear();
    }

    // -----------------------------------------------------------------------
    // Wait side
    // -----------------------------------------------------------------------

    /// Wait until the completion has been signalled, then return.
    ///
    /// If `done == COMPLETION_ALL_DONE`, returns immediately without
    /// modifying `done`.  Otherwise decrements `done` by 1.
    ///
    /// **TODO**: replace spin-wait with `schedule()`.
    pub fn wait(&self) {
        // Fast path: already done.
        if self.check_done_and_consume() {
            return;
        }

        // Slow path: enqueue and spin-wait.
        {
            let mut wq = self.wait.lock();
            wq.push_back(CompWaiter {
                task_id: 0,
                woken: false,
            });
        }

        // TODO: call schedule() here.
        loop {
            core::hint::spin_loop();
            if self.check_done_and_consume() {
                return;
            }
        }
    }

    /// Wait with a timeout (in jiffies).
    ///
    /// Returns `true` if the completion was signalled before the
    /// timeout, `false` if the timeout expired.
    ///
    /// **TODO**: replace spin-budget with real timer infrastructure.
    pub fn wait_timeout(&self, timeout_jiffies: u64) -> bool {
        if self.check_done_and_consume() {
            return true;
        }

        let mut budget = timeout_jiffies.saturating_mul(1_000_000);

        loop {
            if self.check_done_and_consume() {
                return true;
            }
            if budget == 0 {
                return false;
            }
            budget -= 1;
            core::hint::spin_loop();
        }
    }

    /// Non-blocking check: return `true` if the completion is done.
    ///
    /// Mirrors `try_wait_for_completion()`.  Does not consume `done`.
    pub fn try_wait(&self) -> bool {
        self.done.load(Ordering::Acquire) > 0
    }

    /// Return `true` if `done > 0` (signalled at least once).
    pub fn done(&self) -> bool {
        self.done.load(Ordering::Relaxed) > 0
    }

    /// Reset the completion to the unsignalled state.
    ///
    /// Mirrors `reinit_completion()`.  Unsafe to call while tasks are
    /// waiting – those tasks will spin forever.  Only call when you
    /// know no one is currently blocked in `wait()`.
    pub fn reinit(&self) {
        self.done.store(0, Ordering::Release);
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// If `done > 0`, consume one unit and return `true`.
    /// If `done == COMPLETION_ALL_DONE`, return `true` without consuming.
    fn check_done_and_consume(&self) -> bool {
        let d = self.done.load(Ordering::Acquire);
        if d == 0 {
            return false;
        }
        if d == COMPLETION_ALL_DONE {
            return true;
        }
        // Try to consume one completion credit.
        self.done
            .fetch_update(Ordering::Acquire, Ordering::Relaxed, |cur| {
                if cur > 0 && cur != COMPLETION_ALL_DONE {
                    Some(cur - 1)
                } else {
                    None
                }
            })
            .is_ok()
            || self.done.load(Ordering::Acquire) == COMPLETION_ALL_DONE
    }
}

impl Default for Completion {
    fn default() -> Self {
        Self::new()
    }
}
