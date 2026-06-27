//! GIOScheduler matching `gio/gioscheduler.h`.
//!
//! Background I/O job queue with main-loop delivery. Jobs are stored in a
//! `Mutex<Vec<IoJob>>`; [`IoScheduler::process_pending`] runs all pending
//! (non-done, non-cancelled) callbacks.
//!
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::prelude::*;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, Once};

/// A single scheduled I/O job.
struct IoJob {
    id: u64,
    callback: Arc<dyn Fn() + Send + Sync>,
    done: bool,
    cancelled: bool,
}

/// I/O job scheduler (`GIOScheduler`).
pub struct IoScheduler {
    jobs: Mutex<Vec<IoJob>>,
    next_id: AtomicU64,
}

impl IoScheduler {
    /// Creates an empty scheduler.
    pub fn new() -> Self {
        Self {
            jobs: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Enqueues a job and returns its id.
    ///
    /// Mirrors `g_io_scheduler_push_job`.
    pub fn push_job(&self, callback: Arc<dyn Fn() + Send + Sync>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.jobs.lock().push(IoJob {
            id,
            callback,
            done: false,
            cancelled: false,
        });
        id
    }

    /// Cancels a job by id. Returns `true` if the job existed and was not done.
    ///
    /// Mirrors `g_io_scheduler_cancel_job`.
    pub fn cancel_job(&self, id: u64) -> bool {
        let mut jobs = self.jobs.lock();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id && !j.done) {
            job.cancelled = true;
            true
        } else {
            false
        }
    }

    /// Cancels every pending job.
    ///
    /// Mirrors `g_io_scheduler_cancel_all_jobs`.
    pub fn cancel_all(&self) {
        let mut jobs = self.jobs.lock();
        for job in jobs.iter_mut() {
            if !job.done {
                job.cancelled = true;
            }
        }
    }

    /// Marks a job done and runs its callback on the main loop.
    ///
    /// Mirrors `g_io_scheduler_job_send_to_mainloop`.
    pub fn job_send_to_mainloop(&self, id: u64) -> bool {
        let mut jobs = self.jobs.lock();
        let Some(job) = jobs.iter_mut().find(|j| j.id == id) else {
            return false;
        };
        if job.cancelled || job.done {
            return false;
        }
        job.done = true;
        let callback = Arc::clone(&job.callback);
        drop(jobs);
        callback();
        true
    }

    /// Runs callbacks for all jobs that are not yet done or cancelled.
    pub fn process_pending(&self) {
        loop {
            let next = {
                let mut jobs = self.jobs.lock();
                let pos = jobs.iter().position(|j| !j.done && !j.cancelled);
                match pos {
                    Some(idx) => {
                        jobs[idx].done = true;
                        Some(Arc::clone(&jobs[idx].callback))
                    }
                    None => None,
                }
            };
            match next {
                Some(callback) => callback(),
                None => break,
            }
        }
    }

    /// Returns the number of jobs still in the queue (including done).
    pub fn job_count(&self) -> usize {
        self.jobs.lock().len()
    }

    /// Returns how many jobs are pending (not done and not cancelled).
    pub fn pending_count(&self) -> usize {
        self.jobs
            .lock()
            .iter()
            .filter(|j| !j.done && !j.cancelled)
            .count()
    }

    /// Removes finished and cancelled jobs from the queue.
    pub fn prune_finished(&self) {
        self.jobs.lock().retain(|j| !j.done && !j.cancelled);
    }
}

impl Default for IoScheduler {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────── global API ─────────────────────────────────

static SCHEDULER: Once<Mutex<IoScheduler>> = Once::new();

fn global_scheduler() -> &'static Mutex<IoScheduler> {
    SCHEDULER.call_once(|| Mutex::new(IoScheduler::new()))
}

/// Enqueues a job on the default scheduler (`g_io_scheduler_push_job`).
pub fn io_scheduler_push_job(callback: Arc<dyn Fn() + Send + Sync>) -> u64 {
    global_scheduler().lock().push_job(callback)
}

/// Cancels a job on the default scheduler (`g_io_scheduler_cancel_job`).
pub fn io_scheduler_cancel_job(id: u64) -> bool {
    global_scheduler().lock().cancel_job(id)
}

/// Cancels all jobs on the default scheduler (`g_io_scheduler_cancel_all_jobs`).
pub fn io_scheduler_cancel_all() {
    global_scheduler().lock().cancel_all();
}

/// Delivers a job to the main loop on the default scheduler
/// (`g_io_scheduler_job_send_to_mainloop`).
pub fn io_scheduler_job_send_to_mainloop(id: u64) -> bool {
    global_scheduler().lock().job_send_to_mainloop(id)
}

/// Processes pending jobs on the default scheduler.
pub fn io_scheduler_process_pending() {
    global_scheduler().lock().process_pending();
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

    #[test]
    fn test_push_and_process_pending() {
        let scheduler = IoScheduler::new();
        let ran = Arc::new(AtomicU32::new(0));
        let ran2 = Arc::clone(&ran);
        scheduler.push_job(Arc::new(move || {
            ran2.fetch_add(1, AtomicOrdering::SeqCst);
        }));
        assert_eq!(scheduler.pending_count(), 1);
        scheduler.process_pending();
        assert_eq!(ran.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(scheduler.pending_count(), 0);
    }

    #[test]
    fn test_cancel_job() {
        let scheduler = IoScheduler::new();
        let ran = Arc::new(AtomicU32::new(0));
        let id = scheduler.push_job(Arc::new({
            let ran = Arc::clone(&ran);
            move || {
                ran.fetch_add(1, AtomicOrdering::SeqCst);
            }
        }));
        assert!(scheduler.cancel_job(id));
        scheduler.process_pending();
        assert_eq!(ran.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn test_cancel_all() {
        let scheduler = IoScheduler::new();
        let ran = Arc::new(AtomicU32::new(0));
        scheduler.push_job(Arc::new({
            let ran = Arc::clone(&ran);
            move || {
                ran.fetch_add(1, AtomicOrdering::SeqCst);
            }
        }));
        scheduler.push_job(Arc::new({
            let ran = Arc::clone(&ran);
            move || {
                ran.fetch_add(1, AtomicOrdering::SeqCst);
            }
        }));
        scheduler.cancel_all();
        scheduler.process_pending();
        assert_eq!(ran.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn test_job_send_to_mainloop() {
        let scheduler = IoScheduler::new();
        let ran = Arc::new(AtomicU32::new(0));
        let id = scheduler.push_job(Arc::new({
            let ran = Arc::clone(&ran);
            move || {
                ran.fetch_add(1, AtomicOrdering::SeqCst);
            }
        }));
        assert!(scheduler.job_send_to_mainloop(id));
        assert_eq!(ran.load(AtomicOrdering::SeqCst), 1);
        assert!(!scheduler.job_send_to_mainloop(id));
    }

    #[test]
    fn test_global_scheduler() {
        let ran = Arc::new(AtomicU32::new(0));
        let id = io_scheduler_push_job(Arc::new({
            let ran = Arc::clone(&ran);
            move || {
                ran.fetch_add(1, AtomicOrdering::SeqCst);
            }
        }));
        io_scheduler_process_pending();
        assert_eq!(ran.load(AtomicOrdering::SeqCst), 1);
        assert!(!io_scheduler_cancel_job(id));
    }
}
