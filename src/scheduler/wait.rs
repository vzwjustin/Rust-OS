//! In-kernel Wait Queue implementation for RustOS
//!
//! Provides a mechanism for threads to block on specific events and be woken up
//! thread-safely by other parts of the kernel.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::interrupts::without_interrupts;

use crate::process::Pid;
use crate::scheduler::{block_process, unblock_process, yield_cpu};

/// Result returned by timed predicate waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitTimeoutResult {
    /// The predicate became true before the timeout expired.
    ConditionMet,
    /// The timeout expired before the predicate became true.
    TimedOut,
}

/// Serial-friendly wait queue diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WaitQueueStats {
    pub waiters: usize,
    pub waits: u64,
    pub predicate_waits: u64,
    pub timeout_waits: u64,
    pub wake_one_calls: u64,
    pub wake_all_calls: u64,
    pub woken: u64,
}

#[derive(Debug, Clone, Copy)]
struct TimeoutWaiter {
    deadline_us: u64,
    queue_id: usize,
    pid: Pid,
}

lazy_static! {
    static ref TIMEOUT_WAITERS: Mutex<Vec<TimeoutWaiter>> = Mutex::new(Vec::new());
}

static TIMEOUT_WAKEUPS: AtomicU64 = AtomicU64::new(0);

fn timeout_waiter_cb() {
    let now = crate::time::uptime_us();
    let mut expired = Vec::new();

    without_interrupts(|| {
        let mut waiters = TIMEOUT_WAITERS.lock();
        let mut i = 0;
        while i < waiters.len() {
            if waiters[i].deadline_us <= now {
                expired.push(waiters.remove(i));
            } else {
                i += 1;
            }
        }
    });

    for waiter in expired {
        let _ = unblock_process(waiter.pid);
        TIMEOUT_WAKEUPS.fetch_add(1, Ordering::Relaxed);
    }
}

fn remove_timeout_waiter(queue_id: usize, pid: Pid) {
    without_interrupts(|| {
        let mut waiters = TIMEOUT_WAITERS.lock();
        waiters.retain(|w| !(w.queue_id == queue_id && w.pid == pid));
    });
}

/// Global timeout diagnostics for wait queues.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WaitTimeoutStats {
    pub pending: usize,
    pub fired: u64,
}

pub fn timeout_stats() -> WaitTimeoutStats {
    without_interrupts(|| WaitTimeoutStats {
        pending: TIMEOUT_WAITERS.lock().len(),
        fired: TIMEOUT_WAKEUPS.load(Ordering::Relaxed),
    })
}

/// A queue of processes waiting for an event.
#[derive(Debug)]
pub struct WaitQueue {
    waiters: Mutex<VecDeque<Pid>>,
    waits: AtomicU64,
    predicate_waits: AtomicU64,
    timeout_waits: AtomicU64,
    wake_one_calls: AtomicU64,
    wake_all_calls: AtomicU64,
    woken: AtomicU64,
}

impl WaitQueue {
    /// Create a new empty wait queue.
    pub const fn new() -> Self {
        Self {
            waiters: Mutex::new(VecDeque::new()),
            waits: AtomicU64::new(0),
            predicate_waits: AtomicU64::new(0),
            timeout_waits: AtomicU64::new(0),
            wake_one_calls: AtomicU64::new(0),
            wake_all_calls: AtomicU64::new(0),
            woken: AtomicU64::new(0),
        }
    }

    /// Block the current process and add it to the wait queue.
    ///
    /// This function must be called with interrupts enabled (or it will deadlock
    /// the CPU since no interrupts means no timer ticks/scheduling).
    pub fn wait(&self) {
        let pid = crate::process::current_pid();
        self.waits.fetch_add(1, Ordering::Relaxed);

        let should_yield = without_interrupts(|| {
            let mut waiters = self.waiters.lock();
            if !waiters.iter().any(|&queued| queued == pid) {
                waiters.push_back(pid);
            }

            if block_process(pid).is_err() {
                waiters.retain(|&queued| queued != pid);
                false
            } else {
                true
            }
        });

        if should_yield {
            yield_cpu();
        }
    }

    /// Wait until `condition` returns true.
    ///
    /// The predicate is evaluated while interrupts are disabled and the wait
    /// queue lock is held, so it must be non-blocking, must not acquire this
    /// wait queue again, and must not call code that can sleep. The ordering is:
    /// disable interrupts, lock queue, check predicate, enqueue current PID,
    /// mark it blocked, recheck predicate, then either dequeue/unblock or yield.
    pub fn wait_event<F>(&self, condition: F)
    where
        F: Fn() -> bool,
    {
        self.predicate_waits.fetch_add(1, Ordering::Relaxed);
        let pid = crate::process::current_pid();

        loop {
            let should_yield = without_interrupts(|| {
                let mut waiters = self.waiters.lock();
                if condition() {
                    return false;
                }

                if !waiters.iter().any(|&queued| queued == pid) {
                    waiters.push_back(pid);
                }

                if block_process(pid).is_err() {
                    waiters.retain(|&queued| queued != pid);
                    return true;
                }

                if condition() {
                    waiters.retain(|&queued| queued != pid);
                    let _ = unblock_process(pid);
                    false
                } else {
                    true
                }
            });

            if !should_yield {
                return;
            }

            yield_cpu();
        }
    }

    /// Wait until `condition` returns true or `timeout_us` elapses.
    ///
    /// The predicate follows the same restrictions as [`wait_event`]: it must be
    /// non-blocking and must not call back into this wait queue.
    pub fn wait_event_timeout<F>(&self, condition: F, timeout_us: u64) -> WaitTimeoutResult
    where
        F: Fn() -> bool,
    {
        self.timeout_waits.fetch_add(1, Ordering::Relaxed);

        if condition() {
            return WaitTimeoutResult::ConditionMet;
        }

        if timeout_us == 0 {
            return WaitTimeoutResult::TimedOut;
        }

        let pid = crate::process::current_pid();
        let queue_id = self as *const Self as usize;
        let deadline_us = crate::time::uptime_us().saturating_add(timeout_us);
        let mut timeout_registered = false;

        loop {
            if condition() {
                if timeout_registered {
                    remove_timeout_waiter(queue_id, pid);
                }
                return WaitTimeoutResult::ConditionMet;
            }

            let now = crate::time::uptime_us();
            if now >= deadline_us {
                without_interrupts(|| {
                    self.waiters.lock().retain(|&queued| queued != pid);
                });
                if timeout_registered {
                    remove_timeout_waiter(queue_id, pid);
                }
                return WaitTimeoutResult::TimedOut;
            }

            let remaining = deadline_us.saturating_sub(now);
            let should_yield = without_interrupts(|| {
                let mut waiters = self.waiters.lock();
                if condition() {
                    return false;
                }

                if !waiters.iter().any(|&queued| queued == pid) {
                    waiters.push_back(pid);
                }

                if !timeout_registered {
                    TIMEOUT_WAITERS.lock().push(TimeoutWaiter {
                        deadline_us,
                        queue_id,
                        pid,
                    });
                    crate::time::schedule_timer(remaining, timeout_waiter_cb);
                    timeout_registered = true;
                }

                if block_process(pid).is_err() {
                    waiters.retain(|&queued| queued != pid);
                    return true;
                }

                if condition() {
                    waiters.retain(|&queued| queued != pid);
                    remove_timeout_waiter(queue_id, pid);
                    let _ = unblock_process(pid);
                    false
                } else {
                    true
                }
            });

            if !should_yield {
                if timeout_registered {
                    remove_timeout_waiter(queue_id, pid);
                }
                return WaitTimeoutResult::ConditionMet;
            }

            yield_cpu();
        }
    }

    /// Wake up a single process from the wait queue.
    pub fn wake_one(&self) {
        self.wake_one_calls.fetch_add(1, Ordering::Relaxed);
        let pid = without_interrupts(|| self.waiters.lock().pop_front());
        if let Some(pid) = pid {
            let _ = unblock_process(pid);
            self.woken.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Wake up all processes in the wait queue.
    pub fn wake_all(&self) {
        self.wake_all_calls.fetch_add(1, Ordering::Relaxed);
        let pids = without_interrupts(|| {
            let mut waiters = self.waiters.lock();
            let mut pids = Vec::new();
            while let Some(pid) = waiters.pop_front() {
                pids.push(pid);
            }
            pids
        });

        for pid in pids {
            let _ = unblock_process(pid);
            self.woken.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Check if the wait queue is empty.
    pub fn is_empty(&self) -> bool {
        without_interrupts(|| self.waiters.lock().is_empty())
    }

    /// Get the number of waiting processes.
    pub fn len(&self) -> usize {
        without_interrupts(|| self.waiters.lock().len())
    }

    /// Return serial-friendly wait queue diagnostics.
    pub fn stats(&self) -> WaitQueueStats {
        without_interrupts(|| WaitQueueStats {
            waiters: self.waiters.lock().len(),
            waits: self.waits.load(Ordering::Relaxed),
            predicate_waits: self.predicate_waits.load(Ordering::Relaxed),
            timeout_waits: self.timeout_waits.load(Ordering::Relaxed),
            wake_one_calls: self.wake_one_calls.load(Ordering::Relaxed),
            wake_all_calls: self.wake_all_calls.load(Ordering::Relaxed),
            woken: self.woken.load(Ordering::Relaxed),
        })
    }
}
