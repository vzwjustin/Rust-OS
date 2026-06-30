//! Completely Fair Scheduler (CFS) — ported from Linux `kernel/sched/fair.c`
//!
//! CFS tracks each task's *virtual runtime* (`vruntime`): a nanosecond counter
//! scaled by the inverse of the task's weight so that lower-weight tasks appear
//! to advance faster.  The run queue is a sorted set keyed by `(vruntime, pid)`
//! (the Pid tiebreaker prevents silent collisions when two tasks have the same
//! vruntime, which would happen with a plain `BTreeMap<u64, Pid>`).
//!
//! Structures:
//!   `SchedEntity`  — per-task CFS metadata (mirrors `struct sched_entity`)
//!   `LoadWeight`   — weight derived from nice/priority (mirrors `struct load_weight`)
//!   `CfsRunQueue`  — per-CPU CFS run queue (mirrors `struct cfs_rq`)
//!
//! Key functions (mirrors Linux naming where possible):
//!   `calc_delta_fair`    — scale a wall-clock delta by a task's weight
//!   `update_curr`        — advance `curr`'s vruntime and update `min_vruntime`
//!   `enqueue_entity`     — add a task to the CFS tree
//!   `dequeue_entity`     — remove a task from the CFS tree
//!   `pick_next_entity`   — choose the leftmost (smallest-vruntime) runnable task
//!   `check_preempt_tick` — decide whether the running task should be preempted
//!   `sched_slice`        — compute a task's ideal wall-clock slice

#![allow(dead_code, unused_variables)]

use super::sched_class::{DequeueFlags, EnqueueFlags, MAX_RT_PRIO};
use crate::process::Pid;
use alloc::collections::BTreeMap;

// ─────────────────────────────────────────────────────────────────────────────
// Nice → weight table  (kernel/sched/core.c  prio_to_weight[])
// ─────────────────────────────────────────────────────────────────────────────

/// Weight for each nice level (-20 … +19).
/// Index 0 = nice -20, index 20 = nice 0, index 39 = nice +19.
pub const PRIO_TO_WEIGHT: [u64; 40] = [
    88761, 71755, 56483, 46273, 36291, // -20 .. -16
    29154, 23254, 18705, 14949, 11916, // -15 .. -11
    9548, 7620, 6100, 4904, 3906, // -10 .. -6
    3121, 2501, 1991, 1586, 1277, //  -5 .. -1
    1024, 820, 655, 526, 423, //   0 ..  4
    335, 272, 215, 172, 137, //   5 ..  9
    110, 87, 70, 56, 45, //  10 .. 14
    36, 29, 23, 18, 15, //  15 .. 19
];

/// Weight for nice 0 (used as the reference)
pub const NICE_0_WEIGHT: u64 = 1024;

/// Precomputed inverse weights (2^32 / weight) to allow multiply-instead-of-divide.
/// Mirrors `prio_to_wmult[]` in Linux.
pub const PRIO_TO_WMULT: [u32; 40] = [
    48388, 59856, 76040, 92818, 118348,
    147320, 184698, 229616, 287308, 360437,
    449829, 563644, 704093, 875809, 1099582,
    1376151, 1717300, 2157191, 2708050, 3363326,
    4194304, 5237765, 6557202, 8166337, 10153587,
    12820798, 15790321, 19976592, 24970740, 31350126,
    39045157, 49367440, 61356676, 76695844, 95443717,
    119304647, 148102320, 186737708, 238609294, 286331153,
];

// ─────────────────────────────────────────────────────────────────────────────
// Scheduler tunable constants  (mirrors kernel/sched/fair.c)
// ─────────────────────────────────────────────────────────────────────────────

/// Minimum granularity in nanoseconds — a task must run at least this long
/// before being preempted.  Linux default: 750 µs.
pub const SCHED_MIN_GRANULARITY_NS: u64 = 750_000;

/// Target scheduling period in nanoseconds — total wall-clock time across which
/// every runnable task gets one slice.  Linux default: 6 ms.
pub const SCHED_LATENCY_NS: u64 = 6_000_000;

/// Minimum scheduling period (enforced when many tasks are runnable).
pub const SCHED_MIN_PERIOD_NS: u64 = SCHED_MIN_GRANULARITY_NS;

/// Wakeup preemption granularity in nanoseconds.  Linux default: 1 ms.
pub const SCHED_WAKEUP_GRANULARITY_NS: u64 = 1_000_000;

// ─────────────────────────────────────────────────────────────────────────────
// LoadWeight
// ─────────────────────────────────────────────────────────────────────────────

/// Per-task (and per-cfs-rq) load weight.
///
/// Mirrors Linux `struct load_weight`.
#[derive(Debug, Clone, Copy)]
pub struct LoadWeight {
    /// Absolute weight (from `PRIO_TO_WEIGHT` table)
    pub weight: u64,
    /// Precomputed `2^32 / weight` for fast division
    pub inv_weight: u32,
}

impl LoadWeight {
    /// Construct from a nice level (-20 .. 19).
    pub fn from_nice(nice: i32) -> Self {
        let idx = (nice + 20).clamp(0, 39) as usize;
        Self {
            weight: PRIO_TO_WEIGHT[idx],
            inv_weight: PRIO_TO_WMULT[idx],
        }
    }

    /// Construct from a kernel priority value (100..139 for CFS).
    pub fn from_prio(prio: i32) -> Self {
        // kernel priority 120 == nice 0; offset by MAX_RT_PRIO(100) + 20
        let nice = prio - MAX_RT_PRIO - 20;
        Self::from_nice(nice)
    }

    /// Scale `delta` nanoseconds by this weight relative to `NICE_0_WEIGHT`.
    ///
    /// result = delta * NICE_0_WEIGHT / self.weight
    ///
    /// Used by `calc_delta_fair` to compute vruntime increments.
    #[inline]
    pub fn scale_inv(&self, delta_ns: u64) -> u64 {
        if self.weight == 0 || self.weight == NICE_0_WEIGHT {
            return delta_ns;
        }
        // Use inv_weight to avoid a 64-bit division
        // Linux uses __calc_delta which does a full 96-bit multiply; we use
        // a simpler approximation that is accurate for reasonable delta sizes.
        let result = (delta_ns as u128 * self.inv_weight as u128) >> 32;
        result as u64
    }
}

impl Default for LoadWeight {
    fn default() -> Self {
        Self::from_nice(0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SchedEntity
// ─────────────────────────────────────────────────────────────────────────────

/// Per-task CFS scheduling entity.
///
/// Mirrors Linux `struct sched_entity` (the non-group parts).
#[derive(Debug, Clone)]
pub struct SchedEntity {
    /// Accumulated virtual runtime in nanoseconds.  Tasks with smaller vruntime
    /// are picked first by the CFS tree.
    pub vruntime: u64,

    /// Timestamp (ns) when the entity most recently started executing.
    pub exec_start: u64,

    /// Total wall-clock nanoseconds this entity has spent on-CPU.
    pub sum_exec_runtime: u64,

    /// `sum_exec_runtime` at the end of the previous time slice.  Used to
    /// compute per-tick CPU time without resetting the accumulator.
    pub prev_sum_exec_runtime: u64,

    /// True while the entity is on the CFS run queue (but not necessarily
    /// running — it might be preempted).
    pub on_rq: bool,

    /// Load weight derived from this task's priority / nice value.
    pub load: LoadWeight,
}

impl SchedEntity {
    pub fn new(nice: i32) -> Self {
        Self {
            vruntime: 0,
            exec_start: 0,
            sum_exec_runtime: 0,
            prev_sum_exec_runtime: 0,
            on_rq: false,
            load: LoadWeight::from_nice(nice),
        }
    }
}

impl Default for SchedEntity {
    fn default() -> Self {
        Self::new(0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CfsRunQueue
// ─────────────────────────────────────────────────────────────────────────────

/// Per-CPU CFS run queue.
///
/// Mirrors Linux `struct cfs_rq`.
///
/// The sorted set is a `BTreeMap<(u64, Pid), ()>` where the key is
/// `(vruntime, pid)`.  Using a tuple key means two tasks with identical
/// vruntimes are never silently merged (a plain `BTreeMap<u64, Pid>` would
/// drop whichever task was inserted second).
#[derive(Debug)]
pub struct CfsRunQueue {
    /// Cumulative load weight of all tasks on this run queue
    pub load: LoadWeight,

    /// Number of runnable tasks
    pub nr_running: u32,

    /// Minimum vruntime of any task in the tree.  New tasks are placed at
    /// `max(0, min_vruntime - sched_latency)` to avoid starvation.
    pub min_vruntime: u64,

    /// Sorted set: (vruntime, pid) → ()
    /// BTreeMap gives O(log n) insert/remove/find-min.
    tasks: BTreeMap<(u64, Pid), ()>,

    /// Map from pid to its current vruntime (for O(log n) removal by pid).
    vruntime_by_pid: BTreeMap<Pid, u64>,

    /// Map from pid to its scheduling entity (vruntime, weights, …)
    entities: BTreeMap<Pid, SchedEntity>,

    /// PID of the currently executing task (if owned by this run queue)
    pub curr: Option<Pid>,

    /// PID of the preferred next task (set by wakeup preemption)
    pub next: Option<Pid>,

    /// PID of the task that was last running (preferred in pick_next if tie)
    pub last: Option<Pid>,
}

impl CfsRunQueue {
    pub fn new() -> Self {
        Self {
            load: LoadWeight::default(),
            nr_running: 0,
            min_vruntime: 0,
            tasks: BTreeMap::new(),
            vruntime_by_pid: BTreeMap::new(),
            entities: BTreeMap::new(),
            curr: None,
            next: None,
            last: None,
        }
    }

    // ── entity access ────────────────────────────────────────────────────────

    /// Register a new scheduling entity for `pid`.  Called when a task is first
    /// created or when it switches to a CFS scheduling policy.
    pub fn add_entity(&mut self, pid: Pid, nice: i32) {
        let mut se = SchedEntity::new(nice);
        // Place new tasks at min_vruntime so they don't starve older ones.
        se.vruntime = self.min_vruntime;
        self.entities.insert(pid, se);
    }

    /// Remove the scheduling entity for `pid`.  Called on task exit or policy
    /// change away from CFS.
    pub fn remove_entity(&mut self, pid: Pid) {
        // Make sure it is not on the tree before we remove the entity.
        self.remove_from_tree(pid);
        self.entities.remove(&pid);
    }

    /// Get an immutable reference to `pid`'s scheduling entity.
    pub fn entity(&self, pid: Pid) -> Option<&SchedEntity> {
        self.entities.get(&pid)
    }

    /// Get a mutable reference to `pid`'s scheduling entity.
    pub fn entity_mut(&mut self, pid: Pid) -> Option<&mut SchedEntity> {
        self.entities.get_mut(&pid)
    }

    // ── tree operations ──────────────────────────────────────────────────────

    /// Insert `pid` into the sorted tree using its current vruntime.
    fn insert_into_tree(&mut self, pid: Pid) {
        if let Some(se) = self.entities.get(&pid) {
            let vr = se.vruntime;
            self.tasks.insert((vr, pid), ());
            self.vruntime_by_pid.insert(pid, vr);
        }
    }

    /// Remove `pid` from the sorted tree (does nothing if not present).
    fn remove_from_tree(&mut self, pid: Pid) {
        if let Some(vr) = self.vruntime_by_pid.remove(&pid) {
            self.tasks.remove(&(vr, pid));
        }
    }

    /// Return the Pid with the smallest vruntime (leftmost entry in the tree),
    /// without removing it.
    pub fn leftmost(&self) -> Option<Pid> {
        self.tasks.keys().next().map(|&(_, pid)| pid)
    }
}

impl Default for CfsRunQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Core CFS algorithms
// ─────────────────────────────────────────────────────────────────────────────

/// Scale a wall-clock delta by the task's weight relative to nice-0.
///
/// Mirrors Linux `calc_delta_fair()` (fair.c).
///
/// ```text
/// vruntime_delta = delta_ns * (NICE_0_WEIGHT / se.load.weight)
/// ```
///
/// Tasks with weight > NICE_0 (nice < 0) accumulate vruntime *slower* and
/// therefore get more CPU time.  Tasks with weight < NICE_0 (nice > 0)
/// accumulate faster.
pub fn calc_delta_fair(delta_ns: u64, se: &SchedEntity) -> u64 {
    se.load.scale_inv(delta_ns)
}

/// Update the current task's vruntime and per-CPU min_vruntime.
///
/// Mirrors Linux `update_curr()` (fair.c).
///
/// `now_ns` is the current monotonic clock in nanoseconds.
pub fn update_curr(cfs_rq: &mut CfsRunQueue, pid: Pid, now_ns: u64) {
    let delta_exec = {
        let se = match cfs_rq.entities.get_mut(&pid) {
            Some(s) => s,
            None => return,
        };
        if se.exec_start == 0 {
            se.exec_start = now_ns;
            return;
        }
        let delta = now_ns.saturating_sub(se.exec_start);
        se.exec_start = now_ns;
        se.sum_exec_runtime = se.sum_exec_runtime.saturating_add(delta);
        delta
    };

    // Compute the vruntime increment scaled by the task's load weight.
    let delta_vruntime = {
        let se = &cfs_rq.entities[&pid];
        calc_delta_fair(delta_exec, se)
    };

    // Remove from tree, update vruntime, re-insert (key changed).
    cfs_rq.remove_from_tree(pid);
    if let Some(se) = cfs_rq.entities.get_mut(&pid) {
        se.vruntime = se.vruntime.saturating_add(delta_vruntime);
    }
    if cfs_rq.curr == Some(pid) {
        // curr is not in the tree while running; do not re-insert yet.
    } else {
        cfs_rq.insert_into_tree(pid);
    }

    // Update min_vruntime: the minimum of (curr.vruntime, leftmost in tree).
    let leftmost_vr = cfs_rq.leftmost().and_then(|p| cfs_rq.entities.get(&p)).map(|se| se.vruntime);
    let curr_vr = cfs_rq.curr.and_then(|p| cfs_rq.entities.get(&p)).map(|se| se.vruntime);
    let new_min = match (leftmost_vr, curr_vr) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => cfs_rq.min_vruntime,
    };
    // min_vruntime must not go backward (Linux invariant).
    if new_min > cfs_rq.min_vruntime {
        cfs_rq.min_vruntime = new_min;
    }
}

/// Add a task to the CFS run queue.
///
/// Mirrors Linux `enqueue_entity()` (fair.c).
pub fn enqueue_entity(cfs_rq: &mut CfsRunQueue, pid: Pid, flags: EnqueueFlags, now_ns: u64) {
    let se = match cfs_rq.entities.get_mut(&pid) {
        Some(s) => s,
        None => return,
    };

    if se.on_rq {
        return; // already enqueued
    }

    // On wakeup, don't let the task start with a vruntime that is far behind
    // min_vruntime (it would get an unfair burst of CPU time).
    if flags.contains(EnqueueFlags::WAKEUP) && !flags.contains(EnqueueFlags::RESTORE) {
        let min = cfs_rq.min_vruntime;
        if se.vruntime < min {
            se.vruntime = min;
        }
    }

    se.exec_start = now_ns;
    se.on_rq = true;

    // Update CFS-level load
    cfs_rq.load.weight = cfs_rq.load.weight.saturating_add(se.load.weight);
    cfs_rq.nr_running += 1;

    cfs_rq.insert_into_tree(pid);
}

/// Remove a task from the CFS run queue.
///
/// Mirrors Linux `dequeue_entity()` (fair.c).
pub fn dequeue_entity(cfs_rq: &mut CfsRunQueue, pid: Pid, flags: DequeueFlags, now_ns: u64) {
    // Account for any remaining runtime.
    update_curr(cfs_rq, pid, now_ns);

    let se = match cfs_rq.entities.get_mut(&pid) {
        Some(s) => s,
        None => return,
    };

    if !se.on_rq {
        return; // not on this run queue
    }

    se.prev_sum_exec_runtime = se.sum_exec_runtime;
    se.on_rq = false;

    // Update CFS-level load
    cfs_rq.load.weight = cfs_rq.load.weight.saturating_sub(se.load.weight);
    if cfs_rq.nr_running > 0 {
        cfs_rq.nr_running -= 1;
    }

    cfs_rq.remove_from_tree(pid);

    // Clear hints
    if cfs_rq.next == Some(pid) {
        cfs_rq.next = None;
    }
    if cfs_rq.last == Some(pid) {
        cfs_rq.last = None;
    }
}

/// Pick the best next entity to run.
///
/// Linux prefers `cfs_rq.next` (set by wakeup) over the leftmost, provided
/// the difference in vruntime is within one `SCHED_WAKEUP_GRANULARITY_NS`.
///
/// Mirrors Linux `pick_next_entity()` (fair.c).
pub fn pick_next_entity(cfs_rq: &CfsRunQueue) -> Option<Pid> {
    let leftmost = cfs_rq.leftmost()?;

    // If there is a "next" hint from a wakeup preemption, prefer it if it
    // hasn't fallen too far behind the leftmost task.
    if let Some(next_pid) = cfs_rq.next {
        if next_pid != leftmost {
            let lv = cfs_rq.entities.get(&leftmost).map(|se| se.vruntime).unwrap_or(0);
            let nv = cfs_rq.entities.get(&next_pid).map(|se| se.vruntime).unwrap_or(u64::MAX);
            if nv <= lv + SCHED_WAKEUP_GRANULARITY_NS {
                return Some(next_pid);
            }
        }
    }

    Some(leftmost)
}

/// Compute the ideal wall-clock time slice for `pid` on `cfs_rq`.
///
/// ```text
/// slice = (sched_period * se.load.weight) / cfs_rq.load.weight
/// ```
///
/// Mirrors Linux `sched_slice()` (fair.c).
pub fn sched_slice(cfs_rq: &CfsRunQueue, pid: Pid) -> u64 {
    if cfs_rq.load.weight == 0 {
        return SCHED_LATENCY_NS;
    }
    let period = sched_period(cfs_rq.nr_running);
    let se_weight = cfs_rq.entities.get(&pid).map(|se| se.load.weight).unwrap_or(NICE_0_WEIGHT);
    // period * se_weight / total_weight
    let slice = (period as u128 * se_weight as u128 / cfs_rq.load.weight as u128) as u64;
    slice.max(SCHED_MIN_GRANULARITY_NS)
}

/// Compute the scheduling period for `nr_running` tasks.
///
/// ```text
/// period = max(sched_latency, nr_running * min_granularity)
/// ```
///
/// Mirrors Linux `sched_period()` (fair.c).
fn sched_period(nr_running: u32) -> u64 {
    let min = SCHED_MIN_GRANULARITY_NS * nr_running as u64;
    SCHED_LATENCY_NS.max(min)
}

/// Decide whether the running task should be preempted by the leftmost task.
///
/// Preemption is triggered when the running task has used its ideal slice.
///
/// Mirrors Linux `check_preempt_tick()` (fair.c).
///
/// Returns `true` if the scheduler should preempt `curr_pid`.
pub fn check_preempt_tick(cfs_rq: &CfsRunQueue, curr_pid: Pid, now_ns: u64) -> bool {
    let se = match cfs_rq.entities.get(&curr_pid) {
        Some(s) => s,
        None => return false,
    };

    // How much wall-clock time has the current task used since it was scheduled?
    let delta_exec = now_ns.saturating_sub(se.exec_start);

    // Preempt if the task has exceeded its ideal slice.
    let ideal_slice = sched_slice(cfs_rq, curr_pid);
    if delta_exec > ideal_slice {
        return true;
    }

    // Also preempt if the leftmost task's vruntime is significantly smaller.
    if let Some(leftmost) = cfs_rq.leftmost() {
        if leftmost != curr_pid {
            let lv = cfs_rq.entities.get(&leftmost).map(|s| s.vruntime).unwrap_or(0);
            let cv = se.vruntime;
            if cv > lv + SCHED_MIN_GRANULARITY_NS {
                return true;
            }
        }
    }

    false
}

// ─────────────────────────────────────────────────────────────────────────────
// CfsSchedClass  (implements SchedClass for the CFS policy)
// ─────────────────────────────────────────────────────────────────────────────

use super::sched_class::SchedClass;
use crate::scheduler::load_balance::RunQueue;

/// The CFS scheduling class singleton.
pub struct CfsSchedClass;

impl SchedClass for CfsSchedClass {
    fn enqueue_task(&self, rq: &mut RunQueue, pid: Pid, flags: EnqueueFlags) {
        let now = rq.clock_ns();
        enqueue_entity(&mut rq.cfs, pid, flags, now);
    }

    fn dequeue_task(&self, rq: &mut RunQueue, pid: Pid, flags: DequeueFlags) {
        let now = rq.clock_ns();
        dequeue_entity(&mut rq.cfs, pid, flags, now);
    }

    fn yield_task(&self, rq: &mut RunQueue) {
        // Move the current task to the back of its virtual-time queue by
        // advancing its vruntime to min_vruntime.
        if let Some(pid) = rq.cfs.curr {
            let min = rq.cfs.min_vruntime;
            if let Some(se) = rq.cfs.entities.get_mut(&pid) {
                if se.vruntime < min {
                    se.vruntime = min;
                }
            }
        }
    }

    fn pick_next_task(&self, rq: &mut RunQueue) -> Option<Pid> {
        let now = rq.clock_ns();
        if let Some(prev) = rq.cfs.curr {
            update_curr(&mut rq.cfs, prev, now);
            rq.cfs.insert_into_tree(prev);
        }
        let next = pick_next_entity(&rq.cfs)?;
        rq.cfs.remove_from_tree(next);
        rq.cfs.last = rq.cfs.curr;
        rq.cfs.curr = Some(next);
        if let Some(se) = rq.cfs.entities.get_mut(&next) {
            se.exec_start = now;
        }
        Some(next)
    }

    fn put_prev_task(&self, rq: &mut RunQueue, pid: Pid) {
        let now = rq.clock_ns();
        update_curr(&mut rq.cfs, pid, now);
        if let Some(se) = rq.cfs.entities.get(&pid) {
            if se.on_rq {
                rq.cfs.insert_into_tree(pid);
            }
        }
    }

    fn select_task_rq(&self, pid: Pid, preferred_cpu: u32, _flags: u32) -> u32 {
        // Simplified: stay on preferred CPU. A real impl would check cache
        // affinity and load across the topology.
        preferred_cpu
    }

    fn task_tick(&self, rq: &mut RunQueue, pid: Pid, _queued: bool, now_ns: u64) {
        update_curr(&mut rq.cfs, pid, now_ns);
        // Preemption decision left to the global scheduler via check_preempt_tick.
    }

    fn get_rr_interval(&self, rq: &RunQueue, pid: Pid) -> u64 {
        sched_slice(&rq.cfs, pid)
    }

    fn prio_changed(&self, rq: &mut RunQueue, pid: Pid, _old_prio: i32) {
        // Re-weight the entity by re-reading its nice value from the process table.
        // We don't have direct access to the PCB here, so we do a no-op and let
        // the caller call `cfs_rq.add_entity(pid, new_nice)` after updating PCB.
    }

    fn switched_to(&self, rq: &mut RunQueue, pid: Pid) {
        // Task has just been moved to CFS. Place it at min_vruntime.
        let min = rq.cfs.min_vruntime;
        if let Some(se) = rq.cfs.entities.get_mut(&pid) {
            se.vruntime = min;
        }
    }

    fn name(&self) -> &'static str {
        "fair"
    }
}

/// Global CFS scheduling class instance.
pub static CFS_SCHED_CLASS: CfsSchedClass = CfsSchedClass;
