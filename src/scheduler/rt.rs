//! Real-time scheduler — ported from Linux `kernel/sched/rt.c`
//!
//! Two real-time policies are supported:
//!
//! * **SCHED_FIFO** — once a task starts running, it continues until it blocks
//!   or explicitly yields.  Equal-priority FIFO tasks do *not* take turns.
//!
//! * **SCHED_RR** — like FIFO, but equal-priority tasks share a configurable
//!   round-robin timeslice (`RR_TIMESLICE_NS`).  When a task exhausts its
//!   timeslice, it is moved to the back of its priority queue.
//!
//! The RT run queue is a pair of priority arrays — one *active*, one *expired*
//! — each containing a 128-bit bitmap and 100 `VecDeque<Pid>` lists.  The
//! bitmap allows O(1) lookup of the highest non-empty priority level.
//!
//! This file mirrors Linux structures:
//!   `struct rt_prio_array` → `RtPrioArray`
//!   `struct rt_rq`         → `RtRunQueue`

#![allow(dead_code, unused_variables)]

use super::sched_class::{DequeueFlags, EnqueueFlags, SchedClass};
use crate::process::Pid;
use crate::scheduler::load_balance::RunQueue;
use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicI32, Ordering};

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Number of RT priority levels (0 is highest, 99 is lowest).
/// Linux uses 0..MAX_RT_PRIO-1 where MAX_RT_PRIO == 100.
pub const RT_PRIO_LEVELS: usize = 100;

/// Default timeslice for SCHED_RR tasks in nanoseconds (100 ms).
/// Mirrors Linux `RR_TIMESLICE` (100 jiffies at HZ=1000).
pub const RR_TIMESLICE_NS: u64 = 100_000_000;

/// Sentinel value meaning "no RT task is running".
pub const RT_PRIO_NONE: i32 = -1;

// ─────────────────────────────────────────────────────────────────────────────
// RtPrioArray
// ─────────────────────────────────────────────────────────────────────────────

/// Priority array for the RT scheduler.
///
/// Mirrors Linux `struct rt_prio_array` (sched/rt.c).
///
/// `bitmap` is a 128-bit value split across two `u64`s (covering 100 priority
/// levels — bit `i` is set when `queue[i]` is non-empty).
/// `queue[0]` is the *highest* priority (Linux convention: lower numeric prio
/// value ⇒ higher urgency).
pub struct RtPrioArray {
    /// Two u64 words give 128 bits; we only use the first 100.
    /// bit i set ⟺ queue[i] is non-empty.
    pub bitmap: [u64; 2],
    /// Per-priority FIFO queues of PIDs.
    pub queue: [VecDeque<Pid>; RT_PRIO_LEVELS],
}

impl RtPrioArray {
    pub fn new() -> Self {
        Self {
            bitmap: [0u64; 2],
            queue: core::array::from_fn(|_| VecDeque::new()),
        }
    }

    // ── bitmap helpers ───────────────────────────────────────────────────────

    fn set_bit(&mut self, prio: usize) {
        debug_assert!(prio < RT_PRIO_LEVELS);
        if prio < 64 {
            self.bitmap[0] |= 1u64 << prio;
        } else {
            self.bitmap[1] |= 1u64 << (prio - 64);
        }
    }

    fn clear_bit(&mut self, prio: usize) {
        debug_assert!(prio < RT_PRIO_LEVELS);
        if prio < 64 {
            self.bitmap[0] &= !(1u64 << prio);
        } else {
            self.bitmap[1] &= !(1u64 << (prio - 64));
        }
    }

    /// Return the highest-urgency (lowest numeric) non-empty priority level,
    /// or `None` if the array is empty.
    pub fn highest_prio(&self) -> Option<usize> {
        if self.bitmap[0] != 0 {
            return Some(self.bitmap[0].trailing_zeros() as usize);
        }
        if self.bitmap[1] != 0 {
            return Some(64 + self.bitmap[1].trailing_zeros() as usize);
        }
        None
    }

    // ── queue operations ─────────────────────────────────────────────────────

    /// Add `pid` to the back of `prio`'s queue.
    pub fn enqueue(&mut self, pid: Pid, prio: usize) {
        debug_assert!(prio < RT_PRIO_LEVELS);
        self.queue[prio].push_back(pid);
        self.set_bit(prio);
    }

    /// Add `pid` to the *front* of `prio`'s queue (head insertion).
    pub fn enqueue_head(&mut self, pid: Pid, prio: usize) {
        debug_assert!(prio < RT_PRIO_LEVELS);
        self.queue[prio].push_front(pid);
        self.set_bit(prio);
    }

    /// Remove `pid` from `prio`'s queue. Returns `true` if found.
    pub fn dequeue(&mut self, pid: Pid, prio: usize) -> bool {
        debug_assert!(prio < RT_PRIO_LEVELS);
        let q = &mut self.queue[prio];
        if let Some(pos) = q.iter().position(|&p| p == pid) {
            q.remove(pos);
            if q.is_empty() {
                self.clear_bit(prio);
            }
            true
        } else {
            false
        }
    }

    /// Peek at the head of `prio`'s queue without removing.
    pub fn peek(&self, prio: usize) -> Option<Pid> {
        self.queue.get(prio)?.front().copied()
    }

    /// Remove and return the head of `prio`'s queue.
    pub fn pop_head(&mut self, prio: usize) -> Option<Pid> {
        let pid = self.queue[prio].pop_front()?;
        if self.queue[prio].is_empty() {
            self.clear_bit(prio);
        }
        Some(pid)
    }

    /// Move `pid` to the back of its queue (used by SCHED_RR timeslice expiry).
    pub fn requeue(&mut self, pid: Pid, prio: usize) {
        self.dequeue(pid, prio);
        self.enqueue(pid, prio);
    }
}

impl Default for RtPrioArray {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-task RT metadata
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime information for one RT task.
#[derive(Debug, Clone)]
pub struct RtTaskInfo {
    /// Current RT priority (0 = highest, 99 = lowest)
    pub prio: u32,

    /// Scheduling policy: SCHED_FIFO or SCHED_RR
    pub policy: super::sched_class::SchedPolicy,

    /// Remaining timeslice in nanoseconds (SCHED_RR only).
    /// Reset to `RR_TIMESLICE_NS` when a new slice begins.
    pub time_slice_remaining: u64,

    /// True while the task is on the RT active array.
    pub on_rq: bool,
}

impl RtTaskInfo {
    pub fn new_fifo(prio: u32) -> Self {
        Self {
            prio,
            policy: super::sched_class::SchedPolicy::Fifo,
            time_slice_remaining: 0,
            on_rq: false,
        }
    }

    pub fn new_rr(prio: u32) -> Self {
        Self {
            prio,
            policy: super::sched_class::SchedPolicy::RoundRobin,
            time_slice_remaining: RR_TIMESLICE_NS,
            on_rq: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RtRunQueue
// ─────────────────────────────────────────────────────────────────────────────

/// Per-CPU RT run queue.
///
/// Mirrors Linux `struct rt_rq` (sched/rt.c).
pub struct RtRunQueue {
    /// Active priority array — tasks that have not yet used their timeslice
    pub active: RtPrioArray,

    /// Number of runnable RT tasks
    pub nr_running: u32,

    /// Highest (lowest-numbered) occupied priority, or `RT_PRIO_NONE` if empty.
    pub highest_prio: AtomicI32,

    /// True if there are RT tasks that could not fit on this CPU.
    pub overloaded: bool,

    /// Per-task metadata (prio, policy, remaining slice).
    task_info: alloc::collections::BTreeMap<Pid, RtTaskInfo>,

    /// PID of the currently running RT task, if any.
    pub curr: Option<Pid>,
}

impl RtRunQueue {
    pub fn new() -> Self {
        Self {
            active: RtPrioArray::new(),
            nr_running: 0,
            highest_prio: AtomicI32::new(RT_PRIO_NONE),
            overloaded: false,
            task_info: alloc::collections::BTreeMap::new(),
            curr: None,
        }
    }

    // ── task registration ────────────────────────────────────────────────────

    /// Register a new RT task (called when policy is set to FIFO or RR).
    pub fn add_task(&mut self, pid: Pid, info: RtTaskInfo) {
        self.task_info.insert(pid, info);
    }

    /// Remove RT metadata for a task (called on exit or policy change to CFS).
    pub fn remove_task_info(&mut self, pid: Pid) {
        self.task_info.remove(&pid);
    }

    pub fn task_info(&self, pid: Pid) -> Option<&RtTaskInfo> {
        self.task_info.get(&pid)
    }

    pub fn task_info_mut(&mut self, pid: Pid) -> Option<&mut RtTaskInfo> {
        self.task_info.get_mut(&pid)
    }

    // ── highest-priority tracking ────────────────────────────────────────────

    fn update_highest_prio(&self) {
        let val = self
            .active
            .highest_prio()
            .map(|p| p as i32)
            .unwrap_or(RT_PRIO_NONE);
        self.highest_prio.store(val, Ordering::Relaxed);
    }

    // ── enqueue / dequeue ────────────────────────────────────────────────────

    /// Add `pid` to the RT active array.
    ///
    /// Mirrors Linux `enqueue_task_rt()` (rt.c).
    pub fn enqueue_task_rt(&mut self, pid: Pid, flags: EnqueueFlags) {
        let info = match self.task_info.get_mut(&pid) {
            Some(i) => i,
            None => return,
        };
        if info.on_rq {
            return;
        }
        let prio = info.prio as usize;
        info.on_rq = true;

        if flags.contains(EnqueueFlags::HEAD) {
            self.active.enqueue_head(pid, prio);
        } else {
            self.active.enqueue(pid, prio);
        }

        self.nr_running += 1;
        self.update_highest_prio();
    }

    /// Remove `pid` from the RT active array.
    ///
    /// Mirrors Linux `dequeue_task_rt()` (rt.c).
    pub fn dequeue_task_rt(&mut self, pid: Pid, _flags: DequeueFlags) {
        let prio = match self.task_info.get_mut(&pid) {
            Some(i) => {
                if !i.on_rq {
                    return;
                }
                i.on_rq = false;
                i.prio as usize
            }
            None => return,
        };

        self.active.dequeue(pid, prio);
        if self.nr_running > 0 {
            self.nr_running -= 1;
        }
        self.update_highest_prio();
    }

    /// Pick the next RT task to run.
    ///
    /// Mirrors Linux `pick_next_task_rt()` (rt.c).
    /// Returns the PID of the highest-priority runnable task without removing
    /// it from the queue (the caller must call `dequeue_task_rt` when it
    /// actually starts running).
    pub fn pick_next_task_rt(&mut self) -> Option<Pid> {
        let hp = self.active.highest_prio()?;

        // Peek — do not pop.  Linux also keeps the task in the array; it is
        // removed only when it blocks or is preempted.
        let pid = self.active.peek(hp)?;

        // Remove from the array while it is running (re-added on preemption).
        self.active.dequeue(pid, hp);
        if let Some(info) = self.task_info.get_mut(&pid) {
            info.on_rq = false;
        }
        if self.nr_running > 0 {
            self.nr_running -= 1;
        }
        self.update_highest_prio();
        self.curr = Some(pid);

        Some(pid)
    }

    /// Called on every scheduler tick for the running RT task.
    ///
    /// For SCHED_RR: decrement the timeslice; when it reaches zero, requeue
    /// the task at the back of its priority level and signal a reschedule.
    ///
    /// For SCHED_FIFO: no preemption (returns `false` always).
    ///
    /// Mirrors Linux `task_tick_rt()` (rt.c).
    ///
    /// Returns `true` if the task should be preempted (SCHED_RR timeslice
    /// expired).
    pub fn task_tick_rt(&mut self, pid: Pid, elapsed_ns: u64) -> bool {
        use super::sched_class::SchedPolicy;

        let (policy, prio, should_preempt) = {
            let info = match self.task_info.get_mut(&pid) {
                Some(i) => i,
                None => return false,
            };
            match info.policy {
                SchedPolicy::Fifo => {
                    // FIFO: no preemption from the timeslice
                    return false;
                }
                SchedPolicy::RoundRobin => {
                    info.time_slice_remaining =
                        info.time_slice_remaining.saturating_sub(elapsed_ns);
                    let expired = info.time_slice_remaining == 0;
                    if expired {
                        // Reset the timeslice for the next round.
                        info.time_slice_remaining = RR_TIMESLICE_NS;
                    }
                    (info.policy, info.prio as usize, expired)
                }
                _ => return false,
            }
        };

        if should_preempt {
            self.requeue_task_rt(pid);
        }
        should_preempt
    }

    /// Move `pid` to the back of its RT priority queue (SCHED_RR timeslice
    /// exhausted).
    ///
    /// Mirrors Linux `requeue_task_rt()` (rt.c).
    pub fn requeue_task_rt(&mut self, pid: Pid) {
        let prio = match self.task_info.get(&pid) {
            Some(i) => i.prio as usize,
            None => return,
        };

        // Re-insert at the tail without going through full enqueue/dequeue.
        // The task is already marked on_rq = false (it was running), so we
        // set it back to on_rq = true and add it to the back of the queue.
        if let Some(info) = self.task_info.get_mut(&pid) {
            info.on_rq = true;
        }
        self.active.enqueue(pid, prio);
        self.nr_running += 1;
        self.update_highest_prio();
        self.curr = None;
    }

    /// Get the RR interval for `pid` in nanoseconds.
    pub fn rr_interval(&self, pid: Pid) -> u64 {
        match self.task_info.get(&pid) {
            Some(info) => match info.policy {
                super::sched_class::SchedPolicy::RoundRobin => RR_TIMESLICE_NS,
                _ => 0,
            },
            None => 0,
        }
    }
}

impl Default for RtRunQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RtSchedClass  (implements SchedClass for FIFO / RR policies)
// ─────────────────────────────────────────────────────────────────────────────

pub struct RtSchedClass;

impl SchedClass for RtSchedClass {
    fn enqueue_task(&self, rq: &mut RunQueue, pid: Pid, flags: EnqueueFlags) {
        rq.rt.enqueue_task_rt(pid, flags);
    }

    fn dequeue_task(&self, rq: &mut RunQueue, pid: Pid, flags: DequeueFlags) {
        rq.rt.dequeue_task_rt(pid, flags);
    }

    fn yield_task(&self, rq: &mut RunQueue) {
        // For SCHED_RR: move curr to back of its priority queue.
        // For SCHED_FIFO: a yield is honoured by re-queuing at back as well.
        if let Some(pid) = rq.rt.curr {
            rq.rt.requeue_task_rt(pid);
        }
    }

    fn pick_next_task(&self, rq: &mut RunQueue) -> Option<Pid> {
        rq.rt.pick_next_task_rt()
    }

    fn put_prev_task(&self, rq: &mut RunQueue, pid: Pid) {
        // Re-queue the preempted task at the head of its priority level.
        // It should still have remaining timeslice; the head insertion means
        // it will be preferred over newly woken tasks at the same priority.
        if let Some(info) = rq.rt.task_info.get_mut(&pid) {
            info.on_rq = true;
        }
        let prio = rq
            .rt
            .task_info
            .get(&pid)
            .map(|i| i.prio as usize)
            .unwrap_or(0);
        rq.rt.active.enqueue_head(pid, prio);
        rq.rt.nr_running += 1;
        rq.rt.update_highest_prio();
        rq.rt.curr = None;
    }

    fn select_task_rq(&self, pid: Pid, preferred_cpu: u32, _flags: u32) -> u32 {
        // Simplified: stay on the preferred (current) CPU.
        // A real implementation would find the CPU with no higher-priority RT task.
        preferred_cpu
    }

    fn task_tick(&self, rq: &mut RunQueue, pid: Pid, _queued: bool, _now_ns: u64) {
        // We don't know the exact elapsed time here; use a coarse 1 ms tick.
        const TICK_NS: u64 = 1_000_000;
        rq.rt.task_tick_rt(pid, TICK_NS);
    }

    fn get_rr_interval(&self, rq: &RunQueue, pid: Pid) -> u64 {
        rq.rt.rr_interval(pid)
    }

    fn prio_changed(&self, rq: &mut RunQueue, pid: Pid, old_prio: i32) {
        // Re-insert at the new priority level.
        // Simplified: dequeue with the old prio, update the stored prio, re-enqueue.
        if let Some(info) = rq.rt.task_info.get(&pid) {
            if info.on_rq {
                let old = old_prio as usize;
                rq.rt.active.dequeue(pid, old);
                let new_prio = info.prio as usize;
                rq.rt.active.enqueue(pid, new_prio);
                rq.rt.update_highest_prio();
            }
        }
    }

    fn switched_to(&self, rq: &mut RunQueue, pid: Pid) {
        // Task just became an RT task. If it is runnable, preempt CFS curr.
        // (The global scheduler handles the actual context switch; we just
        // ensure the task is enqueued.)
        if rq.rt.task_info.get(&pid).map(|i| !i.on_rq).unwrap_or(false) {
            rq.rt.enqueue_task_rt(pid, EnqueueFlags::empty());
        }
    }

    fn name(&self) -> &'static str {
        "rt"
    }
}

/// Global RT scheduling class instance.
pub static RT_SCHED_CLASS: RtSchedClass = RtSchedClass;
