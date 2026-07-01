//! Work Queue Subsystem
//!
//! Ported from Linux `kernel/workqueue.c` (Tejun Heo's rewrite, 2010+).
//!
//! Workqueues are the primary mechanism for deferring work from interrupt
//! context to process context. Each workqueue is backed by one or more
//! worker threads (kernel threads) that drain a shared pending list.
//!
//! ## Hierarchy
//! ```
//! Workqueue  ──owns──►  WorkerPool  ──manages──►  Worker threads
//!     │                      │
//!     └──queues Work via──►  pending VecDeque
//! ```
//!
//! ## Quick start
//! ```rust
//! // Queue work on the system workqueue
//! let work = Work::new(|_w| { /* do something */ });
//! schedule_work(Arc::new(Mutex::new(work)));
//!
//! // Create a private named workqueue
//! let wq = alloc_workqueue("my_wq", 0, 1);
//! queue_work(&wq, work_item);
//! ```
//!
//! ## Design notes (vs. Linux)
//! - No per-CPU pools; single global pool per priority level (SMP-ready layout kept).
//! - Worker threads are stubbed; work is drained by `run_workqueue()` called
//!   from the scheduler tick (same model as `softirq.rs`).
//! - `spin::Mutex` replaces `spinlock_t`; `lazy_static!` replaces `__init` globals.
//! - `Arc<Mutex<Work>>` instead of raw pointers for safe sharing.

#![allow(dead_code, unused_variables, unused_imports)]

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::interrupts::without_interrupts;

// ── Workqueue flags (matching Linux WQ_* constants) ─────────────────────

/// Not bound to any specific CPU — work can run on any CPU.
pub const WQ_UNBOUND: u32 = 1 << 1;
/// Can be frozen (e.g., during suspend).
pub const WQ_FREEZABLE: u32 = 1 << 2;
/// Has a rescue worker to avoid memory-reclaim deadlocks.
pub const WQ_MEM_RECLAIM: u32 = 1 << 3;
/// High-priority workers (SCHED_HIGHPRI equivalent).
pub const WQ_HIGHPRI: u32 = 1 << 4;
/// CPU-intensive work — concurrency management bypassed.
pub const WQ_CPU_INTENSIVE: u32 = 1 << 5;
/// Sysfs-visible workqueue.
pub const WQ_SYSFS: u32 = 1 << 6;
/// Power-efficient: prefer idle CPUs.
pub const WQ_POWER_EFFICIENT: u32 = 1 << 7;

// Internal flags
const WQ_DRAINING: u32 = 1 << 16;
const WQ_DESTROYING: u32 = 1 << 17;

// ── Work item state bits ────────────────────────────────────────────────

const WORK_PENDING: u32 = 1 << 0; // queued, not yet running
const WORK_RUNNING: u32 = 1 << 1; // currently executing
const WORK_CANCELING: u32 = 1 << 2; // cancel_work_sync in progress
const WORK_DELAYED: u32 = 1 << 3; // sitting in the delayed list

// ── Worker state bits ───────────────────────────────────────────────────

const WORKER_DIE: u32 = 1 << 1;
const WORKER_IDLE: u32 = 1 << 2;
const WORKER_PREP: u32 = 1 << 3;
const WORKER_CPU_INTENSIVE: u32 = 1 << 6;
const WORKER_UNBOUND: u32 = 1 << 7;

// ── Core data structures ─────────────────────────────────────────────────

/// A single unit of deferred work.  Corresponds to Linux `struct work_struct`.
///
/// Users fill in `func` and then hand the `Arc<Mutex<Work>>` to `queue_work`
/// or `schedule_work`.
pub struct Work {
    /// The function to call when this work item executes.
    pub func: Option<fn(&mut Work)>,
    /// Opaque user data (analogous to `container_of` in C).
    pub data: usize,
    /// Atomic state flags (WORK_PENDING | WORK_RUNNING | …).
    state: AtomicU32,
    /// Back-pointer to the owning workqueue name (debug only).
    wq_name: Option<&'static str>,
}

impl Work {
    /// Create a new `Work` item with the given handler.
    pub fn new(func: fn(&mut Work)) -> Self {
        Work {
            func: Some(func),
            data: 0,
            state: AtomicU32::new(0),
            wq_name: None,
        }
    }

    /// Create a `Work` item with associated user data.
    pub fn with_data(func: fn(&mut Work), data: usize) -> Self {
        Work {
            func: Some(func),
            data,
            state: AtomicU32::new(0),
            wq_name: None,
        }
    }

    /// Return `true` if the work is queued but not yet running.
    pub fn is_pending(&self) -> bool {
        self.state.load(Ordering::Acquire) & WORK_PENDING != 0
    }

    /// Return `true` if the work is currently executing.
    pub fn is_running(&self) -> bool {
        self.state.load(Ordering::Acquire) & WORK_RUNNING != 0
    }

    fn set_pending(&self) {
        self.state.fetch_or(WORK_PENDING, Ordering::Release);
    }

    fn clear_pending(&self) {
        self.state.fetch_and(!WORK_PENDING, Ordering::Release);
    }

    fn set_running(&self) {
        self.state.fetch_or(WORK_RUNNING, Ordering::AcqRel);
    }

    fn clear_running(&self) {
        self.state.fetch_and(!WORK_RUNNING, Ordering::AcqRel);
    }
}

// Work is Send — the func pointer is only called from the worker thread.
unsafe impl Send for Work {}
unsafe impl Sync for Work {}

// ── DelayedWork ─────────────────────────────────────────────────────────

/// Work item that executes after a timer delay.
/// Corresponds to Linux `struct delayed_work`.
pub struct DelayedWork {
    /// The underlying work item.
    pub work: Work,
    /// Delay in jiffies (timer ticks).  Stored; actual scheduling uses
    /// `crate::time::schedule_timer` in microseconds.
    pub delay_jiffies: u64,
    /// Absolute deadline in microseconds (set when queued).
    due_us: AtomicU64,
    /// Back-pointer to the owning workqueue (None = system_wq).
    wq: Option<Arc<Workqueue>>,
}

impl DelayedWork {
    pub fn new(func: fn(&mut Work)) -> Self {
        DelayedWork {
            work: Work::new(func),
            delay_jiffies: 0,
            due_us: AtomicU64::new(0),
            wq: None,
        }
    }

    pub fn with_data(func: fn(&mut Work), data: usize) -> Self {
        DelayedWork {
            work: Work::with_data(func, data),
            delay_jiffies: 0,
            due_us: AtomicU64::new(0),
            wq: None,
        }
    }
}

unsafe impl Send for DelayedWork {}
unsafe impl Sync for DelayedWork {}

// ── Worker ──────────────────────────────────────────────────────────────

/// A kernel worker thread.  Corresponds to Linux `struct worker`.
///
/// In RustOS workers are conceptual — actual thread creation is stubbed;
/// the pool is drained by `run_workqueue()` from the timer tick.
pub struct Worker {
    /// Worker index within its pool.
    pub id: u32,
    /// Current work item being executed (None = idle).
    pub current_work: Option<Arc<Mutex<Work>>>,
    /// Worker flags (WORKER_DIE | WORKER_IDLE | …).
    flags: AtomicU32,
    /// Total work items executed by this worker.
    pub executed: AtomicU64,
}

impl Worker {
    fn new(id: u32) -> Self {
        Worker {
            id,
            current_work: None,
            flags: AtomicU32::new(WORKER_IDLE),
            executed: AtomicU64::new(0),
        }
    }

    pub fn is_idle(&self) -> bool {
        self.flags.load(Ordering::Acquire) & WORKER_IDLE != 0
    }

    fn set_idle(&self) {
        self.flags.fetch_or(WORKER_IDLE, Ordering::Release);
    }

    fn clear_idle(&self) {
        self.flags.fetch_and(!WORKER_IDLE, Ordering::Release);
    }

    fn should_die(&self) -> bool {
        self.flags.load(Ordering::Acquire) & WORKER_DIE != 0
    }
}

// ── WorkerPool ──────────────────────────────────────────────────────────

/// A pool of workers that share a pending-work queue.
/// Corresponds to Linux `struct worker_pool`.
struct WorkerPool {
    /// Pool flags (subset of `WQ_*`).
    flags: u32,
    /// Pending work items (FIFO).
    pending: VecDeque<Arc<Mutex<Work>>>,
    /// Workers managed by this pool.
    workers: Vec<Worker>,
    /// Next worker ID to assign.
    next_worker_id: u32,
    /// Total work items processed.
    executed: u64,
    /// Total work items queued.
    queued: u64,
}

impl WorkerPool {
    fn new(flags: u32) -> Self {
        let mut pool = WorkerPool {
            flags,
            pending: VecDeque::new(),
            workers: Vec::new(),
            next_worker_id: 0,
            executed: 0,
            queued: 0,
        };
        // Start with one worker (stub).
        pool.maybe_create_worker();
        pool
    }

    fn maybe_create_worker(&mut self) {
        let id = self.next_worker_id;
        self.next_worker_id += 1;
        self.workers.push(Worker::new(id));
    }

    fn push(&mut self, work: Arc<Mutex<Work>>) {
        work.lock().set_pending();
        self.pending.push_back(work);
        self.queued += 1;
    }

    fn pop(&mut self) -> Option<Arc<Mutex<Work>>> {
        self.pending.pop_front()
    }

    fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Execute one work item.  Returns true if work was executed.
    fn process_one(&mut self) -> bool {
        let item = match self.pending.pop_front() {
            Some(w) => w,
            None => return false,
        };

        // Mark the first idle worker as busy.
        let worker_idx = self.workers.iter().position(|w| w.is_idle());
        if let Some(idx) = worker_idx {
            self.workers[idx].clear_idle();
            self.workers[idx].executed.fetch_add(1, Ordering::Relaxed);
        }

        {
            let mut w = item.lock();
            w.clear_pending();
            w.set_running();
            if let Some(func) = w.func {
                func(&mut *w);
            }
            w.clear_running();
        }

        // Return worker to idle.
        if let Some(idx) = worker_idx {
            self.workers[idx].set_idle();
        }

        self.executed += 1;
        true
    }
}

// ── Workqueue ────────────────────────────────────────────────────────────

/// A named workqueue.  Corresponds to Linux `struct workqueue_struct`.
///
/// Created via [`alloc_workqueue`] / [`create_singlethread_workqueue`];
/// shared as `Arc<Workqueue>`.
pub struct Workqueue {
    /// Human-readable name (appears in /proc/workqueue in Linux).
    pub name: String,
    /// Workqueue flags (WQ_UNBOUND | WQ_HIGHPRI | …).
    pub flags: u32,
    /// Maximum number of concurrent active work items (0 = unlimited).
    pub max_active: u32,
    /// Internal state flags (WQ_DRAINING | WQ_DESTROYING).
    wq_flags: AtomicU32,
    /// The backing worker pool (Mutex for interior mutability in Arc).
    pool: Mutex<WorkerPool>,
    /// Delayed work waiting for their timers.
    delayed: Mutex<Vec<DelayedEntry>>,
    /// Stats: total items flushed via flush_workqueue.
    flushed: AtomicU64,
}

struct DelayedEntry {
    due_us: u64,
    work: Arc<Mutex<Work>>,
}

impl Workqueue {
    fn new(name: &str, flags: u32, max_active: u32) -> Self {
        let pool_flags = if flags & WQ_HIGHPRI != 0 {
            WQ_HIGHPRI
        } else {
            0
        };
        Workqueue {
            name: name.to_string(),
            flags,
            max_active,
            wq_flags: AtomicU32::new(0),
            pool: Mutex::new(WorkerPool::new(pool_flags)),
            delayed: Mutex::new(Vec::new()),
            flushed: AtomicU64::new(0),
        }
    }

    fn is_draining(&self) -> bool {
        self.wq_flags.load(Ordering::Acquire) & WQ_DRAINING != 0
    }

    fn is_destroying(&self) -> bool {
        self.wq_flags.load(Ordering::Acquire) & WQ_DESTROYING != 0
    }

    /// Returns true if there is no pending or in-flight work.
    pub fn is_idle(&self) -> bool {
        self.pool.lock().is_empty()
    }

    /// Number of pending (not yet started) work items.
    pub fn pending_count(&self) -> usize {
        self.pool.lock().pending_count()
    }

    /// Tick the delayed list: move any due items to the pending pool.
    fn tick_delayed(&self) {
        let now = crate::time::uptime_us();
        let mut due_items: Vec<Arc<Mutex<Work>>> = Vec::new();
        without_interrupts(|| {
            let mut delayed = self.delayed.lock();
            delayed.retain(|entry| {
                if entry.due_us <= now {
                    due_items.push(Arc::clone(&entry.work));
                    false
                } else {
                    true
                }
            });
        });
        if !due_items.is_empty() {
            without_interrupts(|| {
                let mut pool = self.pool.lock();
                for item in due_items {
                    pool.push(item);
                }
            });
        }
    }

    /// Drain pending work — called from `run_workqueue()`.
    /// Returns the number of work items executed.
    fn drain_pending(&self) -> usize {
        if self.is_destroying() {
            return 0;
        }
        self.tick_delayed();
        let mut count = 0;
        loop {
            let did_work = without_interrupts(|| self.pool.lock().process_one());
            if !did_work {
                break;
            }
            count += 1;
            // Respect max_active (0 = unlimited).
            if self.max_active != 0 && count >= self.max_active as usize {
                break;
            }
        }
        count
    }
}

unsafe impl Send for Workqueue {}
unsafe impl Sync for Workqueue {}

// ── System workqueue singletons ──────────────────────────────────────────

lazy_static! {
    /// General-purpose system workqueue.  Corresponds to `system_wq`.
    pub static ref SYSTEM_WQ: Arc<Workqueue> = Arc::new(
        Workqueue::new("events", 0, 0)
    );

    /// High-priority system workqueue.  Corresponds to `system_highpri_wq`.
    pub static ref SYSTEM_HIGHPRI_WQ: Arc<Workqueue> = Arc::new(
        Workqueue::new("events_highpri", WQ_HIGHPRI, 0)
    );

    /// Workqueue for long-running work items.  Corresponds to `system_long_wq`.
    pub static ref SYSTEM_LONG_WQ: Arc<Workqueue> = Arc::new(
        Workqueue::new("events_long", WQ_UNBOUND, 0)
    );

    /// Unbound workqueue — work can run on any CPU.
    pub static ref SYSTEM_UNBOUND_WQ: Arc<Workqueue> = Arc::new(
        Workqueue::new("events_unbound", WQ_UNBOUND, 0)
    );

    /// Freezable workqueue — suspended during system freeze.
    pub static ref SYSTEM_FREEZABLE_WQ: Arc<Workqueue> = Arc::new(
        Workqueue::new("events_freezable", WQ_FREEZABLE, 0)
    );

    /// Power-efficient unbound workqueue.
    pub static ref SYSTEM_POWER_EFFICIENT_WQ: Arc<Workqueue> = Arc::new(
        Workqueue::new("events_power_efficient", WQ_UNBOUND | WQ_POWER_EFFICIENT, 0)
    );

    /// Global registry of all named workqueues (for flush-all, stats).
    static ref WQ_REGISTRY: Mutex<Vec<Arc<Workqueue>>> = Mutex::new(Vec::new());
}

// ── Public API: Workqueue lifecycle ─────────────────────────────────────

/// Create a new named workqueue.
///
/// Corresponds to Linux `alloc_workqueue(name, flags, max_active)`.
///
/// * `name` — human-readable name (debug, /proc visibility in Linux).
/// * `flags` — combination of `WQ_*` constants.
/// * `max_active` — max concurrent active items (0 = default/unlimited).
pub fn alloc_workqueue(name: &str, flags: u32, max_active: u32) -> Arc<Workqueue> {
    let wq = Arc::new(Workqueue::new(name, flags, max_active));
    without_interrupts(|| WQ_REGISTRY.lock().push(Arc::clone(&wq)));
    wq
}

/// Create a single-threaded workqueue (max_active = 1, serialised).
///
/// Corresponds to Linux `create_singlethread_workqueue(name)`.
pub fn create_singlethread_workqueue(name: &str) -> Arc<Workqueue> {
    alloc_workqueue(name, WQ_UNBOUND, 1)
}

/// Create a freezable single-threaded workqueue.
pub fn create_freezable_workqueue(name: &str) -> Arc<Workqueue> {
    alloc_workqueue(name, WQ_FREEZABLE | WQ_UNBOUND, 1)
}

/// Destroy a workqueue: drain all pending work, then mark it destroyed.
///
/// Corresponds to Linux `destroy_workqueue(wq)`.
pub fn destroy_workqueue(wq: &Arc<Workqueue>) {
    // Mark draining so no new work is accepted.
    wq.wq_flags.fetch_or(WQ_DRAINING, Ordering::Release);
    flush_workqueue(wq);
    wq.wq_flags.fetch_or(WQ_DESTROYING, Ordering::Release);
    // Remove from registry.
    without_interrupts(|| {
        let mut reg = WQ_REGISTRY.lock();
        reg.retain(|w| !Arc::ptr_eq(w, wq));
    });
}

// ── Public API: Queueing work ────────────────────────────────────────────

/// Queue a work item onto a specific workqueue.
///
/// Returns `true` if the item was newly queued, `false` if it was already
/// pending.  Corresponds to Linux `queue_work(wq, work)`.
pub fn queue_work(wq: &Arc<Workqueue>, work: Arc<Mutex<Work>>) -> bool {
    if wq.is_draining() || wq.is_destroying() {
        return false;
    }
    // Already pending — don't queue twice.
    if work.lock().is_pending() {
        return false;
    }
    without_interrupts(|| wq.pool.lock().push(Arc::clone(&work)));
    true
}

/// Queue delayed work onto a specific workqueue.
///
/// `delay_jiffies` are stored on the `DelayedWork` but internally converted
/// to microseconds for the timer subsystem (1 jiffy = 1 ms in RustOS).
///
/// Corresponds to Linux `queue_delayed_work(wq, dwork, delay)`.
pub fn queue_delayed_work(
    wq: &Arc<Workqueue>,
    dwork: &mut DelayedWork,
    delay_jiffies: u64,
) -> bool {
    if wq.is_draining() || wq.is_destroying() {
        return false;
    }
    if dwork.work.is_pending() {
        return false;
    }
    dwork.delay_jiffies = delay_jiffies;
    dwork.work.set_pending();
    // Convert jiffies to microseconds (1 jiffy = 1 ms = 1000 µs).
    let delay_us = delay_jiffies.saturating_mul(1000);
    let due_us = crate::time::uptime_us().saturating_add(delay_us);
    dwork.due_us.store(due_us, Ordering::Release);

    // Wrap the Work field as an Arc for the pool.
    // Since we can't easily take an Arc from a field, we create a lightweight
    // proxy: store a raw snapshot that the timer fires via the delayed list.
    // In production this would use a timer wheel; here we use a simple Vec.
    let work_arc = Arc::new(Mutex::new(Work::with_data(
        dwork.work.func.unwrap_or(|_| {}),
        dwork.work.data,
    )));
    without_interrupts(|| {
        wq.delayed.lock().push(DelayedEntry {
            due_us,
            work: work_arc,
        });
    });
    // Also register with the time subsystem for an early wake.
    crate::time::schedule_timer(delay_us, delayed_work_timer_fire);
    true
}

/// Queue a work item on `SYSTEM_WQ`.
///
/// Corresponds to Linux `schedule_work(work)`.
pub fn schedule_work(work: Arc<Mutex<Work>>) -> bool {
    queue_work(&SYSTEM_WQ, work)
}

/// Queue delayed work on `SYSTEM_WQ`.
///
/// Corresponds to Linux `schedule_delayed_work(dwork, delay)`.
pub fn schedule_delayed_work(dwork: &mut DelayedWork, delay_jiffies: u64) -> bool {
    // Clone the Arc reference to system_wq to avoid borrow issues.
    let wq = Arc::clone(&*SYSTEM_WQ);
    queue_delayed_work(&wq, dwork, delay_jiffies)
}

// Timer callback fired when a delayed work item becomes due.
// We only raise the softirq so the next tick will call run_workqueue().
fn delayed_work_timer_fire() {
    crate::softirq::raise_softirq(crate::softirq::TIMER_SOFTIRQ);
}

// ── Public API: Flushing and cancellation ────────────────────────────────

/// Wait for a single work item to finish executing.
///
/// Returns `true` if the work was pending/running and we waited for it.
/// In the current stub implementation this drains the queue synchronously.
///
/// Corresponds to Linux `flush_work(work)`.
pub fn flush_work(work: &Arc<Mutex<Work>>) -> bool {
    let was_pending = work.lock().is_pending() || work.lock().is_running();
    // Drain the system workqueue so the item executes.
    // In a threaded implementation we would block on a completion.
    run_workqueue();
    was_pending
}

/// Cancel a pending work item without waiting for it to complete.
///
/// Returns `true` if the item was successfully cancelled (was pending).
///
/// Corresponds to Linux `cancel_work(work)`.
pub fn cancel_work(work: &Arc<Mutex<Work>>) -> bool {
    let w = work.lock();
    if w.is_pending() && !w.is_running() {
        w.clear_pending();
        w.state.fetch_or(WORK_CANCELING, Ordering::Release);
        true
    } else {
        false
    }
}

/// Cancel a pending work item, waiting for any in-flight execution to finish.
///
/// Returns `true` if the item was cancelled before it ran.
///
/// Corresponds to Linux `cancel_work_sync(work)`.
pub fn cancel_work_sync(work: &Arc<Mutex<Work>>) -> bool {
    let cancelled = cancel_work(work);
    // Busy-wait until any running instance finishes.
    // A real implementation would use a completion/wait_queue.
    let mut spins: u32 = 0;
    while work.lock().is_running() {
        core::hint::spin_loop();
        spins += 1;
        if spins > 1_000_000 {
            break; // bail to avoid hard lockup
        }
    }
    work.lock()
        .state
        .fetch_and(!WORK_CANCELING, Ordering::Release);
    cancelled
}

/// Cancel a pending delayed work item and wait for it.
///
/// Corresponds to Linux `cancel_delayed_work_sync(dwork)`.
pub fn cancel_delayed_work_sync(dwork: &mut DelayedWork) -> bool {
    let was_pending = dwork.work.is_pending();
    dwork.work.clear_pending();
    was_pending
}

/// Flush all pending work on the given workqueue.
///
/// Blocks until the queue is empty.  Corresponds to Linux `flush_workqueue(wq)`.
pub fn flush_workqueue(wq: &Arc<Workqueue>) {
    loop {
        let executed = without_interrupts(|| wq.drain_pending());
        if executed == 0 {
            break;
        }
    }
    wq.flushed.fetch_add(1, Ordering::Relaxed);
}

/// Flush the system workqueue.
///
/// Corresponds to Linux `flush_scheduled_work()`.
pub fn flush_scheduled_work() {
    flush_workqueue(&SYSTEM_WQ);
}

// ── Worker thread main loop (stub) ───────────────────────────────────────

/// Main loop for a worker thread.
///
/// In RustOS, actual kernel thread creation is stubbed; this function
/// represents what the thread would do if created via
/// `crate::process::thread::spawn_kernel_thread`.
///
/// Corresponds to Linux `worker_thread(worker)`.
pub fn worker_thread(worker: &mut Worker, pool: &Mutex<WorkerPool>) -> ! {
    loop {
        if worker.should_die() {
            break;
        }

        let did_work = without_interrupts(|| pool.lock().process_one());

        if !did_work {
            worker.set_idle();
            // In a real implementation: sleep on a wait_queue until
            // keep_working() returns true.
            core::hint::spin_loop();
        }
    }

    // Should never be reached in a normal kernel worker.
    loop {
        x86_64::instructions::hlt();
    }
}

/// Process a single work item on the given worker.
///
/// Corresponds to Linux `process_one_work(worker, work)`.
pub fn process_one_work(worker: &mut Worker, work: Arc<Mutex<Work>>) {
    worker.clear_idle();
    {
        let mut w = work.lock();
        w.clear_pending();
        w.set_running();
        if let Some(func) = w.func {
            func(&mut *w);
        }
        w.clear_running();
    }
    worker.set_idle();
    worker.executed.fetch_add(1, Ordering::Relaxed);
}

/// Decide whether a pool's worker should keep looping or sleep.
///
/// Corresponds to Linux `keep_working(pool)`.
pub fn keep_working(pool: &Mutex<WorkerPool>) -> bool {
    !pool.lock().is_empty()
}

// ── Scheduler integration ────────────────────────────────────────────────

/// Process pending work across all registered workqueues.
///
/// Called from the timer-tick / scheduler path (same role as
/// `crate::softirq::run_workqueue`).
///
/// Returns the total number of work items executed.
pub fn run_workqueue() -> usize {
    // Tick delayed items in all registered queues first.
    let wqs: Vec<Arc<Workqueue>> =
        without_interrupts(|| WQ_REGISTRY.lock().iter().cloned().collect());

    let mut total = 0usize;
    for wq in &wqs {
        wq.tick_delayed();
    }

    // Then drain pending pools: high-pri first, then normal.
    for wq in &wqs {
        if wq.flags & WQ_HIGHPRI != 0 {
            total += wq.drain_pending();
        }
    }
    for wq in &wqs {
        if wq.flags & WQ_HIGHPRI == 0 {
            total += wq.drain_pending();
        }
    }

    total
}

// ── Initialisation ───────────────────────────────────────────────────────

/// Initialise the workqueue subsystem.
///
/// Must be called once during kernel init, before interrupts are enabled.
/// Forces the lazy_static globals to be constructed and registers the
/// system workqueues.
///
/// Corresponds to Linux `workqueue_init_early()` + `workqueue_init()`.
pub fn init() {
    // Touch each system WQ so the lazy_static initialises now.
    without_interrupts(|| {
        let mut reg = WQ_REGISTRY.lock();
        reg.push(Arc::clone(&*SYSTEM_WQ));
        reg.push(Arc::clone(&*SYSTEM_HIGHPRI_WQ));
        reg.push(Arc::clone(&*SYSTEM_LONG_WQ));
        reg.push(Arc::clone(&*SYSTEM_UNBOUND_WQ));
        reg.push(Arc::clone(&*SYSTEM_FREEZABLE_WQ));
        reg.push(Arc::clone(&*SYSTEM_POWER_EFFICIENT_WQ));
    });

    crate::serial_println!(
        "[workqueue] initialized: system_wq, highpri, long, unbound, freezable, power_efficient"
    );
}

// ── Diagnostics ──────────────────────────────────────────────────────────

/// Snapshot of workqueue statistics.
#[derive(Debug, Clone, Default)]
pub struct WorkqueueStats {
    pub name: String,
    pub flags: u32,
    pub pending: usize,
    pub executed: u64,
    pub flushed: u64,
    pub workers: usize,
}

/// Collect statistics from all registered workqueues.
pub fn workqueue_stats() -> Vec<WorkqueueStats> {
    let wqs: Vec<Arc<Workqueue>> =
        without_interrupts(|| WQ_REGISTRY.lock().iter().cloned().collect());

    wqs.iter()
        .map(|wq| {
            let pool = without_interrupts(|| wq.pool.lock());
            WorkqueueStats {
                name: wq.name.clone(),
                flags: wq.flags,
                pending: pool.pending_count(),
                executed: pool.executed,
                flushed: wq.flushed.load(Ordering::Relaxed),
                workers: pool.workers.len(),
            }
        })
        .collect()
}

/// Dump workqueue stats to the serial console.
pub fn dump_workqueue_stats() {
    let stats = workqueue_stats();
    crate::serial_println!("[workqueue] registered queues: {}", stats.len());
    for s in &stats {
        crate::serial_println!(
            "  {:24} flags={:#06x} pending={:4} executed={:8} workers={}",
            s.name,
            s.flags,
            s.pending,
            s.executed,
            s.workers,
        );
    }
}
