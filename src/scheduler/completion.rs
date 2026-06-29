//! In-kernel Completion Variables for RustOS
//!
//! Completion variables are a synchronization primitive used when one thread
//! needs to wait for some activity to complete on another thread/CPU.

use core::sync::atomic::{AtomicU64, Ordering};

use spin::Mutex;
use x86_64::instructions::interrupts::without_interrupts;

use crate::scheduler::wait::{WaitQueue, WaitTimeoutResult};

const COMPLETE_ALL_COUNT: u32 = u32::MAX / 2;

/// Serial-friendly completion diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompletionStats {
    pub done: u32,
    pub waiters: usize,
    pub waits: u64,
    pub timeout_waits: u64,
    pub completions: u64,
    pub complete_all_calls: u64,
    pub consumed: u64,
}

/// A completion variable.
#[derive(Debug)]
pub struct Completion {
    done: Mutex<u32>,
    wait_queue: WaitQueue,
    waits: AtomicU64,
    timeout_waits: AtomicU64,
    completions: AtomicU64,
    complete_all_calls: AtomicU64,
    consumed: AtomicU64,
}

impl Completion {
    /// Create a new uncompleted completion variable.
    pub const fn new() -> Self {
        Self {
            done: Mutex::new(0),
            wait_queue: WaitQueue::new(),
            waits: AtomicU64::new(0),
            timeout_waits: AtomicU64::new(0),
            completions: AtomicU64::new(0),
            complete_all_calls: AtomicU64::new(0),
            consumed: AtomicU64::new(0),
        }
    }

    fn try_consume(&self) -> bool {
        without_interrupts(|| {
            let mut done = self.done.lock();
            if *done == 0 {
                false
            } else {
                *done = done.saturating_sub(1);
                self.consumed.fetch_add(1, Ordering::Relaxed);
                true
            }
        })
    }

    /// Wait for the completion to be signaled.
    ///
    /// If it is already completed, this returns immediately.
    pub fn wait_for_completion(&self) {
        self.waits.fetch_add(1, Ordering::Relaxed);
        loop {
            if self.try_consume() {
                return;
            }

            self.wait_queue
                .wait_event(|| without_interrupts(|| *self.done.lock() > 0));
        }
    }

    /// Wait up to `timeout_us` microseconds for a completion.
    ///
    /// Returns `true` only if this call consumed a completion count.
    pub fn wait_for_completion_timeout(&self, timeout_us: u64) -> bool {
        self.timeout_waits.fetch_add(1, Ordering::Relaxed);
        let deadline = crate::time::uptime_us().saturating_add(timeout_us);

        loop {
            if self.try_consume() {
                return true;
            }

            let now = crate::time::uptime_us();
            if now >= deadline {
                return self.try_consume();
            }

            let remaining = deadline.saturating_sub(now);
            match self
                .wait_queue
                .wait_event_timeout(|| without_interrupts(|| *self.done.lock() > 0), remaining)
            {
                WaitTimeoutResult::ConditionMet => {
                    if self.try_consume() {
                        return true;
                    }
                }
                WaitTimeoutResult::TimedOut => return self.try_consume(),
            }
        }
    }

    /// Non-blocking completion consume helper.
    pub fn try_wait_for_completion(&self) -> bool {
        self.try_consume()
    }

    /// Signal that the activity has completed.
    ///
    /// Wakes up a single waiting thread.
    pub fn complete(&self) {
        without_interrupts(|| {
            let mut done = self.done.lock();
            *done = done.saturating_add(1);
        });
        self.completions.fetch_add(1, Ordering::Relaxed);
        self.wait_queue.wake_one();
    }

    /// Signal that the activity has completed and wake all waiters.
    pub fn complete_all(&self) {
        without_interrupts(|| {
            let mut done = self.done.lock();
            *done = COMPLETE_ALL_COUNT;
        });
        self.complete_all_calls.fetch_add(1, Ordering::Relaxed);
        self.wait_queue.wake_all();
    }

    /// Reset the completion variable to the uncompleted state.
    pub fn reinit(&self) {
        without_interrupts(|| {
            let mut done = self.done.lock();
            *done = 0;
        });
    }

    /// Check if the completion has occurred.
    pub fn is_done(&self) -> bool {
        without_interrupts(|| *self.done.lock() > 0)
    }

    /// Return the current completion count.
    pub fn done_count(&self) -> u32 {
        without_interrupts(|| *self.done.lock())
    }

    /// Return serial-friendly completion diagnostics.
    pub fn stats(&self) -> CompletionStats {
        CompletionStats {
            done: self.done_count(),
            waiters: self.wait_queue.len(),
            waits: self.waits.load(Ordering::Relaxed),
            timeout_waits: self.timeout_waits.load(Ordering::Relaxed),
            completions: self.completions.load(Ordering::Relaxed),
            complete_all_calls: self.complete_all_calls.load(Ordering::Relaxed),
            consumed: self.consumed.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn completion_counts_are_consumed_one_at_a_time() {
        let completion = Completion::new();
        completion.complete();
        completion.complete();

        assert_eq!(completion.done_count(), 2);
        assert!(completion.try_wait_for_completion());
        assert_eq!(completion.done_count(), 1);
        assert!(completion.try_wait_for_completion());
        assert_eq!(completion.done_count(), 0);
        assert!(!completion.try_wait_for_completion());
    }

    #[test_case]
    fn completion_complete_all_persists_until_reinit() {
        let completion = Completion::new();
        completion.complete_all();

        assert!(completion.done_count() >= COMPLETE_ALL_COUNT - 1);
        assert!(completion.try_wait_for_completion());
        assert!(completion.is_done());

        completion.reinit();
        assert_eq!(completion.done_count(), 0);
        assert!(!completion.try_wait_for_completion());
    }
}
