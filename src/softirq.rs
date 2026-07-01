//! SoftIRQ, Tasklet, and Workqueue subsystem
//!
//! Ported from Linux kernel/softirq.c and kernel/workqueue.c.
//! Provides deferred work execution: softirqs run in interrupt context,
//! tasklets are serialized softirqs, and this kernel's workqueue is drained
//! explicitly by scheduler/tick paths.
//!
//! ## SoftIRQ types
//! HI, TIMER, NET_TX, NET_RX, BLOCK, IRQ_POLL, TASKLET, SCHED, HRTIMER, RCU
//!
//! ## Usage
//! - `register_softirq(idx, handler)` — install a handler
//! - `raise_softirq(idx)` — mark pending (called from IRQ handler)
//! - `do_softirq()` — process pending softirqs (called on IRQ exit)
//! - `schedule_work(work)` — queue work for deferred execution
//! - `tasklet_init / tasklet_schedule` — schedule a tasklet

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;
use x86_64::instructions::interrupts::without_interrupts;

// ── SoftIRQ indices ─────────────────────────────────────────────────────

pub const HI_SOFTIRQ: usize = 0;
pub const TIMER_SOFTIRQ: usize = 1;
pub const NET_TX_SOFTIRQ: usize = 2;
pub const NET_RX_SOFTIRQ: usize = 3;
pub const BLOCK_SOFTIRQ: usize = 4;
pub const IRQ_POLL_SOFTIRQ: usize = 5;
pub const TASKLET_SOFTIRQ: usize = 6;
pub const SCHED_SOFTIRQ: usize = 7;
pub const HRTIMER_SOFTIRQ: usize = 8;
pub const RCU_SOFTIRQ: usize = 9;
pub const NR_SOFTIRQS: usize = 10;

pub static SOFTIRQ_NAMES: [&str; NR_SOFTIRQS] = [
    "HI", "TIMER", "NET_TX", "NET_RX", "BLOCK", "IRQ_POLL", "TASKLET", "SCHED", "HRTIMER", "RCU",
];

const MAX_SOFTIRQ_RESTART: u32 = 10;

// ── SoftIRQ action ──────────────────────────────────────────────────────

pub type SoftIrqHandler = fn();

#[derive(Clone, Copy)]
struct SoftIrqAction {
    handler: Option<SoftIrqHandler>,
}

// ── Per-CPU softirq state (single-CPU for now; SMP-ready layout) ────────

struct SoftIrqCpuState {
    pending: u32,
    bh_disable_count: u32,
    vec: [SoftIrqAction; NR_SOFTIRQS],
    stats: [u64; NR_SOFTIRQS],
}

impl SoftIrqCpuState {
    const fn new() -> Self {
        Self {
            pending: 0,
            bh_disable_count: 0,
            vec: [SoftIrqAction { handler: None }; NR_SOFTIRQS],
            stats: [0; NR_SOFTIRQS],
        }
    }
}

static SOFTIRQ_CPU: Mutex<SoftIrqCpuState> = Mutex::new(SoftIrqCpuState::new());

/// Global softirq pending flag — checked by IRQ exit path
static SOFTIRQ_PENDING: AtomicBool = AtomicBool::new(false);

// ── Tasklet ─────────────────────────────────────────────────────────────

pub struct Tasklet {
    func: Option<fn(usize)>,
    data: usize,
    state: AtomicU32,
}

const TASKLET_STATE_SCHED: u32 = 0;
const TASKLET_STATE_RUN: u32 = 1;

static TASKLET_VEC: Mutex<VecDeque<Box<Tasklet>>> = Mutex::new(VecDeque::new());
static HI_TASKLET_VEC: Mutex<VecDeque<Box<Tasklet>>> = Mutex::new(VecDeque::new());

impl Tasklet {
    pub fn new(func: fn(usize), data: usize) -> Self {
        Self {
            func: Some(func),
            data,
            state: AtomicU32::new(0),
        }
    }

    fn is_scheduled(&self) -> bool {
        self.state.load(Ordering::Acquire) & (1 << TASKLET_STATE_SCHED) != 0
    }

    fn schedule(&self) {
        self.state
            .fetch_or(1 << TASKLET_STATE_SCHED, Ordering::Release);
    }

    fn clear_sched(&self) {
        self.state
            .fetch_and(!(1 << TASKLET_STATE_SCHED), Ordering::Release);
    }
}

// ── Workqueue ───────────────────────────────────────────────────────────

pub struct WorkStruct {
    func: Option<fn(&mut WorkStruct)>,
    data: usize,
    queued: AtomicBool,
}

impl WorkStruct {
    pub const fn new(func: fn(&mut WorkStruct), data: usize) -> Self {
        Self {
            func: Some(func),
            data,
            queued: AtomicBool::new(false),
        }
    }

    pub fn init(&mut self, func: fn(&mut WorkStruct), data: usize) {
        self.func = Some(func);
        self.data = data;
        self.queued.store(false, Ordering::Release);
    }
}

struct WorkQueue {
    name: String,
    normal: VecDeque<Box<WorkStruct>>,
    high_pri: VecDeque<Box<WorkStruct>>,
    executed: u64,
}

static WORKQUEUE: Mutex<Option<WorkQueue>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DelayedWorkStats {
    pub pending: usize,
    pub scheduled: u64,
    pub timer_fires: u64,
    pub moved_to_workqueue: u64,
}

struct DelayedWorkEntry {
    due_us: u64,
    work: Box<WorkStruct>,
}

static DELAYED_WORK: Mutex<Vec<DelayedWorkEntry>> = Mutex::new(Vec::new());
static DELAYED_WORK_SCHEDULED: AtomicU64 = AtomicU64::new(0);
static DELAYED_WORK_TIMER_FIRES: AtomicU64 = AtomicU64::new(0);
static DELAYED_WORK_MOVED: AtomicU64 = AtomicU64::new(0);

// ── Initialization ──────────────────────────────────────────────────────

pub fn init() {
    without_interrupts(|| {
        let mut wq = WORKQUEUE.lock();
        *wq = Some(WorkQueue {
            name: String::from("system_wq"),
            normal: VecDeque::new(),
            high_pri: VecDeque::new(),
            executed: 0,
        });
    });

    // Register tasklet softirq handlers
    register_softirq(HI_SOFTIRQ, tasklet_hi_action);
    register_softirq(TIMER_SOFTIRQ, timer_softirq_action);
    register_softirq(TASKLET_SOFTIRQ, tasklet_action);
    register_softirq(
        SCHED_SOFTIRQ,
        crate::scheduler::load_balance::sched_softirq_action,
    );

    crate::serial_println!("[softirq] initialized ({} softirq types)", NR_SOFTIRQS);
}

// ── SoftIRQ API ─────────────────────────────────────────────────────────

/// Register a softirq handler for the given index.
/// Must be called during init, before interrupts are enabled.
pub fn register_softirq(idx: usize, handler: SoftIrqHandler) {
    assert!(idx < NR_SOFTIRQS, "softirq index out of range");
    without_interrupts(|| {
        let mut cpu = SOFTIRQ_CPU.lock();
        cpu.vec[idx].handler = Some(handler);
    });
}

/// Raise a softirq — mark it pending for processing on IRQ exit.
/// Safe to call from interrupt context.
pub fn raise_softirq(idx: usize) {
    assert!(idx < NR_SOFTIRQS);
    without_interrupts(|| {
        let mut cpu = SOFTIRQ_CPU.lock();
        cpu.pending |= 1u32 << idx;
    });
    SOFTIRQ_PENDING.store(true, Ordering::Release);
}

/// Check if any softirqs are pending.
pub fn softirq_pending() -> bool {
    SOFTIRQ_PENDING.load(Ordering::Acquire)
}

/// Disable bottom-half processing (softirqs).
pub fn local_bh_disable() {
    without_interrupts(|| {
        let mut cpu = SOFTIRQ_CPU.lock();
        cpu.bh_disable_count += 1;
    });
}

/// Enable bottom-half processing. If softirqs are pending and we're
/// not in interrupt context, process them.
pub fn local_bh_enable() {
    let should_process = without_interrupts(|| {
        let mut cpu = SOFTIRQ_CPU.lock();
        if cpu.bh_disable_count > 0 {
            cpu.bh_disable_count -= 1;
        }
        cpu.bh_disable_count == 0 && cpu.pending != 0
    });

    if should_process {
        do_softirq();
    }
}

/// Process all pending softirqs. Called from IRQ exit or local_bh_enable.
/// Runs up to MAX_SOFTIRQ_RESTART times to drain pending work.
pub fn do_softirq() {
    for _restart in 0..MAX_SOFTIRQ_RESTART {
        let pending = without_interrupts(|| SOFTIRQ_CPU.lock().pending);
        if pending == 0 {
            SOFTIRQ_PENDING.store(false, Ordering::Release);
            return;
        }

        // Process each pending softirq in priority order (bit 0 = highest)
        for bit in 0..NR_SOFTIRQS {
            let mask = 1u32 << bit;
            if pending & mask == 0 {
                continue;
            }

            // Clear the pending bit and run the handler
            let handler = without_interrupts(|| {
                let mut cpu = SOFTIRQ_CPU.lock();
                cpu.pending &= !mask;
                cpu.stats[bit] += 1;
                cpu.vec[bit].handler
            });

            if let Some(h) = handler {
                h();
            }
        }

        // Re-check if more softirqs were raised during processing
        let still_pending = without_interrupts(|| SOFTIRQ_CPU.lock().pending);
        if still_pending == 0 {
            SOFTIRQ_PENDING.store(false, Ordering::Release);
            return;
        }
    }

    // Exceeded max restarts — leave pending for ksoftirqd
    crate::serial_println!("[softirq] max restarts exceeded, deferring");
}

/// Get softirq statistics for debugging.
pub fn softirq_stats() -> [(usize, &'static str, u64); NR_SOFTIRQS] {
    without_interrupts(|| {
        let cpu = SOFTIRQ_CPU.lock();
        let mut result = [(0usize, "", 0u64); NR_SOFTIRQS];
        for i in 0..NR_SOFTIRQS {
            result[i] = (i, SOFTIRQ_NAMES[i], cpu.stats[i]);
        }
        result
    })
}

// ── Tasklet API ─────────────────────────────────────────────────────────

/// Tasklet action for TASKLET_SOFTIRQ (normal priority).
fn tasklet_action() {
    loop {
        let tasklet = without_interrupts(|| TASKLET_VEC.lock().pop_front());
        let Some(t) = tasklet else { break };
        t.clear_sched();
        if let Some(func) = t.func {
            func(t.data);
        }
    }
}

/// Tasklet action for HI_SOFTIRQ (high priority).
fn tasklet_hi_action() {
    loop {
        let tasklet = without_interrupts(|| HI_TASKLET_VEC.lock().pop_front());
        let Some(t) = tasklet else { break };
        t.clear_sched();
        if let Some(func) = t.func {
            func(t.data);
        }
    }
}

fn delayed_work_timer_cb() {
    raise_softirq(TIMER_SOFTIRQ);
}

fn timer_softirq_action() {
    DELAYED_WORK_TIMER_FIRES.fetch_add(1, Ordering::Relaxed);
    let now = crate::time::uptime_us();
    let mut due = Vec::new();

    without_interrupts(|| {
        let mut delayed = DELAYED_WORK.lock();
        let mut i = 0;
        while i < delayed.len() {
            if delayed[i].due_us <= now {
                due.push(delayed.remove(i).work);
            } else {
                i += 1;
            }
        }
    });

    for work in due {
        DELAYED_WORK_MOVED.fetch_add(1, Ordering::Relaxed);
        schedule_work(work);
    }
}

/// Schedule a tasklet for normal-priority execution.
pub fn tasklet_schedule(tasklet: Box<Tasklet>) {
    if tasklet.is_scheduled() {
        return;
    }
    tasklet.schedule();
    without_interrupts(|| TASKLET_VEC.lock().push_back(tasklet));
    raise_softirq(TASKLET_SOFTIRQ);
}

/// Schedule a tasklet for high-priority execution (runs in HI softirq).
pub fn tasklet_hi_schedule(tasklet: Box<Tasklet>) {
    if tasklet.is_scheduled() {
        return;
    }
    tasklet.schedule();
    without_interrupts(|| HI_TASKLET_VEC.lock().push_back(tasklet));
    raise_softirq(HI_SOFTIRQ);
}

// ── Workqueue API ───────────────────────────────────────────────────────

/// Queue a work item on the system workqueue.
pub fn schedule_work(work: Box<WorkStruct>) {
    if work.queued.load(Ordering::Acquire) {
        return;
    }
    work.queued.store(true, Ordering::Release);
    without_interrupts(|| {
        let mut wq = WORKQUEUE.lock();
        if let Some(w) = wq.as_mut() {
            w.normal.push_back(work);
        }
    });
    // Workqueue processing happens on next scheduler tick via run_workqueue()
}

/// Queue a high-priority work item.
pub fn schedule_work_high_pri(work: Box<WorkStruct>) {
    if work.queued.load(Ordering::Acquire) {
        return;
    }
    work.queued.store(true, Ordering::Release);
    without_interrupts(|| {
        let mut wq = WORKQUEUE.lock();
        if let Some(w) = wq.as_mut() {
            w.high_pri.push_back(work);
        }
    });
}

/// Process pending workqueue items. Called from the scheduler tick.
/// Returns the number of work items executed.
pub fn run_workqueue() -> usize {
    let mut executed = 0usize;

    // Process high-priority work first
    loop {
        let work = {
            without_interrupts(|| {
                let mut wq = WORKQUEUE.lock();
                wq.as_mut().and_then(|w| w.high_pri.pop_front())
            })
        };
        let Some(mut work) = work else { break };

        work.queued.store(false, Ordering::Release);
        if let Some(func) = work.func {
            func(&mut work);
        }
        executed += 1;
    }

    // Then normal-priority work
    loop {
        let work = {
            without_interrupts(|| {
                let mut wq = WORKQUEUE.lock();
                wq.as_mut().and_then(|w| w.normal.pop_front())
            })
        };
        let Some(mut work) = work else { break };

        work.queued.store(false, Ordering::Release);
        if let Some(func) = work.func {
            func(&mut work);
        }
        executed += 1;
    }

    if executed > 0 {
        without_interrupts(|| {
            if let Some(w) = WORKQUEUE.lock().as_mut() {
                w.executed += executed as u64;
            }
        });
    }

    executed
}

/// Get total work items executed (for stats).
pub fn workqueue_executed_count() -> u64 {
    without_interrupts(|| WORKQUEUE.lock().as_ref().map(|w| w.executed).unwrap_or(0))
}

/// Check if workqueue has pending items.
pub fn workqueue_pending() -> bool {
    without_interrupts(|| {
        WORKQUEUE
            .lock()
            .as_ref()
            .map(|w| !w.normal.is_empty() || !w.high_pri.is_empty())
            .unwrap_or(false)
    })
}

/// Queue a work item to become runnable after `delay_ms` milliseconds.
pub fn schedule_delayed_work_ms(work: Box<WorkStruct>, delay_ms: u64) {
    if work.queued.load(Ordering::Acquire) {
        return;
    }

    let delay_us = delay_ms.saturating_mul(1000);
    let due_us = crate::time::uptime_us().saturating_add(delay_us);
    without_interrupts(|| {
        DELAYED_WORK.lock().push(DelayedWorkEntry { due_us, work });
    });
    DELAYED_WORK_SCHEDULED.fetch_add(1, Ordering::Relaxed);
    crate::time::schedule_timer(delay_us, delayed_work_timer_cb);
}

/// Return serial-friendly delayed work diagnostics.
pub fn delayed_work_stats() -> DelayedWorkStats {
    without_interrupts(|| DelayedWorkStats {
        pending: DELAYED_WORK.lock().len(),
        scheduled: DELAYED_WORK_SCHEDULED.load(Ordering::Relaxed),
        timer_fires: DELAYED_WORK_TIMER_FIRES.load(Ordering::Relaxed),
        moved_to_workqueue: DELAYED_WORK_MOVED.load(Ordering::Relaxed),
    })
}

// ── Deferred work convenience ───────────────────────────────────────────

/// A simple delayed work wrapper that combines a WorkStruct with a timer.
pub struct DelayedWork {
    work: WorkStruct,
    delay_ms: u64,
    submit_time: AtomicU64,
}

impl DelayedWork {
    pub fn new(func: fn(&mut WorkStruct), data: usize, delay_ms: u64) -> Self {
        Self {
            work: WorkStruct::new(func, data),
            delay_ms,
            submit_time: AtomicU64::new(0),
        }
    }

    /// Schedule this delayed work. It becomes runnable after `delay_ms` milliseconds.
    pub fn schedule(self) {
        let now = crate::time::uptime_ns();
        self.submit_time.store(now, Ordering::Release);
        schedule_delayed_work_ms(Box::new(self.work), self.delay_ms);
    }
}

// ── IRQ-safe helpers ────────────────────────────────────────────────────

/// Called from the interrupt exit path. If softirqs are pending,
/// process them.
pub fn irq_exit() {
    if softirq_pending() {
        do_softirq();
    }
}
