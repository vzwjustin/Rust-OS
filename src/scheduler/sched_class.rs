//! Scheduling class hierarchy — mirrors Linux `kernel/sched/sched.h`
//!
//! Linux defines a linked list of `sched_class` structs ordered by priority
//! (deadline → RT → fair → idle).  Here we model the same contract as a Rust
//! trait so each policy can be implemented in its own module while the core
//! scheduler dispatches through a single vtable.
//!
//! Note: the schedulable entity throughout RustOS is a `Pid` (u32), not an
//! `Arc<Task>` as in Linux.  Every method that touches "the current task"
//! therefore accepts a `Pid`.

#![allow(dead_code, unused_variables)]

use crate::process::Pid;

// ─────────────────────────────────────────────────────────────────────────────
// Scheduling policy constants  (mirrors <linux/sched.h>)
// ─────────────────────────────────────────────────────────────────────────────

pub const SCHED_NORMAL: u32 = 0; // CFS, UNIX time-sharing
pub const SCHED_FIFO: u32 = 1; // real-time, no preemption by equal priority
pub const SCHED_RR: u32 = 2; // real-time, round-robin within priority
pub const SCHED_BATCH: u32 = 3; // CFS, batch (no interactive boost)
pub const SCHED_IDLE: u32 = 5; // CFS, lowest possible weight
pub const SCHED_DEADLINE: u32 = 6; // earliest-deadline-first (constant, no impl)

// ─────────────────────────────────────────────────────────────────────────────
// Priority arithmetic  (mirrors kernel/sched/sched.h)
// ─────────────────────────────────────────────────────────────────────────────

/// Highest nice value (least CPU-hungry)
pub const MAX_NICE: i32 = 19;
/// Lowest nice value (most CPU-hungry)
pub const MIN_NICE: i32 = -20;
/// Total width of the nice range
pub const NICE_WIDTH: i32 = 40;
/// Real-time priorities occupy 0..MAX_RT_PRIO
pub const MAX_RT_PRIO: i32 = 100;
/// Total priority range: RT ∪ nice
pub const MAX_PRIO: i32 = MAX_RT_PRIO + NICE_WIDTH;
/// Default priority (nice 0)
pub const DEFAULT_PRIO: i32 = MAX_RT_PRIO + NICE_WIDTH / 2;

/// Convert a nice value (-20..19) to a kernel priority (100..139)
#[inline]
pub fn nice_to_prio(nice: i32) -> i32 {
    MAX_RT_PRIO + nice + 20
}

/// Convert a kernel priority (100..139) to a nice value
#[inline]
pub fn prio_to_nice(prio: i32) -> i32 {
    prio - MAX_RT_PRIO - 20
}

// ─────────────────────────────────────────────────────────────────────────────
// Enqueue / dequeue flags  (mirrors kernel/sched/sched.h ENQUEUE_*/DEQUEUE_*)
// ─────────────────────────────────────────────────────────────────────────────

bitflags::bitflags! {
    /// Flags passed to `enqueue_task`
    pub struct EnqueueFlags: u32 {
        /// Task is being woken from sleep
        const WAKEUP       = 1 << 0;
        /// Task was preempted — keep it at the head of its queue
        const HEAD         = 1 << 1;
        /// Task has been migrated from another CPU's run queue
        const MIGRATED     = 1 << 2;
        /// Restore the saved virtual runtime (do not place at current min_vruntime)
        const RESTORE      = 1 << 3;
    }
}

bitflags::bitflags! {
    /// Flags passed to `dequeue_task`
    pub struct DequeueFlags: u32 {
        /// Task is being put to sleep
        const SLEEP        = 1 << 0;
        /// Save virtual runtime so it can be restored on re-enqueue
        const SAVE         = 1 << 1;
        /// Task is being migrated to another run queue
        const MOVE         = 1 << 2;
        /// Task is being dequeued for good (terminating)
        const NOCLOCK      = 1 << 3;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Scheduling policy enum
// ─────────────────────────────────────────────────────────────────────────────

/// Linux-compatible scheduling policy.
///
/// Passed through `sched_setscheduler(2)` and stored in the PCB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedPolicy {
    /// Default time-sharing (CFS)
    Normal,
    /// Real-time FIFO (no preemption between equal-prio tasks)
    Fifo,
    /// Real-time round-robin (equal-prio tasks share a timeslice)
    RoundRobin,
    /// Batch (CFS variant, no interactive boost)
    Batch,
    /// Idle (runs only when nothing else can)
    Idle,
    /// Earliest-deadline-first (reserved; not yet implemented)
    Deadline,
}

impl SchedPolicy {
    /// Return true if this policy belongs to the real-time class.
    pub fn is_realtime(self) -> bool {
        matches!(self, SchedPolicy::Fifo | SchedPolicy::RoundRobin)
    }

    /// Return true if this policy belongs to CFS.
    pub fn is_cfs(self) -> bool {
        matches!(
            self,
            SchedPolicy::Normal | SchedPolicy::Batch | SchedPolicy::Idle
        )
    }
}

impl From<u32> for SchedPolicy {
    fn from(v: u32) -> Self {
        match v {
            SCHED_FIFO => SchedPolicy::Fifo,
            SCHED_RR => SchedPolicy::RoundRobin,
            SCHED_BATCH => SchedPolicy::Batch,
            SCHED_IDLE => SchedPolicy::Idle,
            SCHED_DEADLINE => SchedPolicy::Deadline,
            _ => SchedPolicy::Normal,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SchedClass trait  (mirrors struct sched_class in sched.h)
// ─────────────────────────────────────────────────────────────────────────────

/// A scheduling class: the vtable that distinguishes CFS, RT, idle, etc.
///
/// The `RunQueue` type is defined in `load_balance.rs`; we forward-declare it
/// here as a raw pointer to avoid a circular import.  All impls use the
/// concrete type via `load_balance::RunQueue`.
///
/// Method-level doc mirrors the Linux kernel source.
pub trait SchedClass: Send + Sync {
    // ── queue management ────────────────────────────────────────────────────

    /// Add `pid` to this class's run queue.
    ///
    /// Linux: `enqueue_task()`
    fn enqueue_task(&self, rq: &mut crate::scheduler::load_balance::RunQueue, pid: Pid, flags: EnqueueFlags);

    /// Remove `pid` from this class's run queue.
    ///
    /// Linux: `dequeue_task()`
    fn dequeue_task(&self, rq: &mut crate::scheduler::load_balance::RunQueue, pid: Pid, flags: DequeueFlags);

    /// Called when the current task voluntarily yields the CPU.
    ///
    /// Linux: `yield_task()`
    fn yield_task(&self, rq: &mut crate::scheduler::load_balance::RunQueue);

    // ── task selection ───────────────────────────────────────────────────────

    /// Pick the next task to run.  Returns `None` if the class has nothing to
    /// run (the core scheduler will then fall through to a lower-priority class).
    ///
    /// Linux: `pick_next_task()`
    fn pick_next_task(&self, rq: &mut crate::scheduler::load_balance::RunQueue) -> Option<Pid>;

    /// Notify the class that `pid` is being descheduled (being replaced by the
    /// newly picked task).
    ///
    /// Linux: `put_prev_task()`
    fn put_prev_task(&self, rq: &mut crate::scheduler::load_balance::RunQueue, pid: Pid);

    // ── CPU selection ────────────────────────────────────────────────────────

    /// Select the best CPU to run `pid` on.  Returns a CPU id.
    ///
    /// Linux: `select_task_rq()`
    fn select_task_rq(&self, pid: Pid, preferred_cpu: u32, flags: u32) -> u32;

    // ── tick accounting ──────────────────────────────────────────────────────

    /// Called every scheduler tick for the running task.  `queued` is true when
    /// `pid` was already re-queued before this tick (e.g. it yielded).
    ///
    /// Linux: `task_tick()`
    fn task_tick(
        &self,
        rq: &mut crate::scheduler::load_balance::RunQueue,
        pid: Pid,
        queued: bool,
        now_ns: u64,
    );

    // ── timeslice ────────────────────────────────────────────────────────────

    /// Return the round-robin interval for `pid` in nanoseconds.
    ///
    /// Linux: `get_rr_interval()`
    fn get_rr_interval(&self, rq: &crate::scheduler::load_balance::RunQueue, pid: Pid) -> u64;

    // ── priority changes ─────────────────────────────────────────────────────

    /// Called when `pid`'s priority has changed (e.g. `setpriority(2)`).
    ///
    /// Linux: `prio_changed()`
    fn prio_changed(
        &self,
        rq: &mut crate::scheduler::load_balance::RunQueue,
        pid: Pid,
        old_prio: i32,
    );

    /// Called after `pid` has been moved to this scheduling class.
    ///
    /// Linux: `switched_to()`
    fn switched_to(&self, rq: &mut crate::scheduler::load_balance::RunQueue, pid: Pid);

    // ── class name (for debugging) ───────────────────────────────────────────
    fn name(&self) -> &'static str;
}

// ─────────────────────────────────────────────────────────────────────────────
// Static dispatch wrapper
// ─────────────────────────────────────────────────────────────────────────────

/// Dispatch to the correct `SchedClass` implementation given a `SchedPolicy`.
///
/// Linux maintains a singly-linked list of `sched_class` structs ordered from
/// highest to lowest priority.  We replicate that ordering with an enum and
/// explicit priority checks in the core scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClassPriority {
    /// Earliest-deadline-first (highest)
    Deadline = 0,
    /// Real-time (FIFO / RR)
    Rt = 1,
    /// Completely fair scheduler (normal / batch / idle)
    Fair = 2,
    /// Idle thread
    Idle = 3,
}

impl From<SchedPolicy> for ClassPriority {
    fn from(p: SchedPolicy) -> Self {
        match p {
            SchedPolicy::Deadline => ClassPriority::Deadline,
            SchedPolicy::Fifo | SchedPolicy::RoundRobin => ClassPriority::Rt,
            SchedPolicy::Normal | SchedPolicy::Batch => ClassPriority::Fair,
            SchedPolicy::Idle => ClassPriority::Idle,
        }
    }
}
