//! Load balancing — ported from Linux `kernel/sched/core.c` and `topology.c`
//!
//! This module provides:
//!
//! * `RunQueue` — the per-CPU run queue that aggregates the CFS and RT
//!   sub-queues together with global book-keeping.  This is the single struct
//!   that `SchedClass` implementations receive as a `&mut RunQueue`.
//!
//! * `load_balance()` — migrate tasks from the busiest CPU to this one when
//!   this CPU becomes idle or a significant imbalance is detected.
//!
//! * `rebalance_domains()` — outer loop that walks scheduling domains and
//!   triggers `load_balance` at each level (simplified: single domain here).
//!
//! * `trigger_load_balance()` — entry point called from the per-CPU timer tick.
//!
//! Structures mirror Linux:
//!   `struct rq`             → `RunQueue`
//!   `enum cpu_idle_type`    → `CpuIdleType`

#![allow(dead_code, unused_variables)]

use super::cfs::CfsRunQueue;
use super::rt::RtRunQueue;
use super::sched_class::SchedPolicy;
use crate::process::Pid;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// ─────────────────────────────────────────────────────────────────────────────
// CpuIdleType
// ─────────────────────────────────────────────────────────────────────────────

/// CPU idle type — mirrors `enum cpu_idle_type` (sched/sched.h).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuIdleType {
    /// CPU is actively running a task
    NotIdle = 0,
    /// CPU just became idle (has not yet run the idle task)
    NewlyIdle = 1,
    /// CPU is in the idle task
    Idle = 2,
}

// ─────────────────────────────────────────────────────────────────────────────
// RunQueue
// ─────────────────────────────────────────────────────────────────────────────

/// Per-CPU run queue.
///
/// Mirrors Linux `struct rq` (sched/sched.h).
///
/// Owns the sub-queues for every scheduling class.  The scheduler core
/// dispatches to the class with the highest-priority runnable task:
///   RT (highest) → CFS → idle (lowest).
pub struct RunQueue {
    // ── sub-queues ───────────────────────────────────────────────────────────
    /// CFS (fair) sub-queue
    pub cfs: CfsRunQueue,
    /// RT (FIFO / RR) sub-queue
    pub rt: RtRunQueue,

    // ── book-keeping ─────────────────────────────────────────────────────────
    /// Total runnable tasks across all classes
    pub nr_running: AtomicU32,
    /// Total context switches on this CPU
    pub nr_switches: u64,
    /// PID currently occupying the CPU (None ⇒ idle)
    pub curr: Option<Pid>,
    /// PID of the idle task for this CPU
    pub idle: Option<Pid>,
    /// CPU index (0-based)
    pub cpu: u32,

    // ── clock ────────────────────────────────────────────────────────────────
    /// Monotonic clock (nanoseconds) updated on every tick
    pub clock: AtomicU64,
    /// Task-specific clock — same as `clock` but not advanced while the CPU is
    /// running an IRQ handler.  Mirrors `rq->clock_task`.
    pub clock_task: AtomicU64,

    // ── load tracking ────────────────────────────────────────────────────────
    /// Exponentially-weighted moving average of the run-queue length.
    /// Stored as a fixed-point u32 (units: tasks × 2^11).
    pub avg_load: AtomicU32,

    // ── migration ────────────────────────────────────────────────────────────
    /// Number of tasks that have been pushed away during load balance
    pub nr_pushed: u64,
    /// Number of tasks that have been pulled to this CPU during load balance
    pub nr_pulled: u64,
}

impl RunQueue {
    pub fn new(cpu: u32) -> Self {
        Self {
            cfs: CfsRunQueue::new(),
            rt: RtRunQueue::new(),
            nr_running: AtomicU32::new(0),
            nr_switches: 0,
            curr: None,
            idle: None,
            cpu,
            clock: AtomicU64::new(0),
            clock_task: AtomicU64::new(0),
            avg_load: AtomicU32::new(0),
            nr_pushed: 0,
            nr_pulled: 0,
        }
    }

    // ── clock helpers ────────────────────────────────────────────────────────

    /// Return the current monotonic clock in nanoseconds.
    #[inline]
    pub fn clock_ns(&self) -> u64 {
        self.clock.load(Ordering::Relaxed)
    }

    /// Advance the run-queue clock.  Called from the timer interrupt.
    #[inline]
    pub fn update_clock(&self, now_ns: u64) {
        self.clock.store(now_ns, Ordering::Relaxed);
        self.clock_task.store(now_ns, Ordering::Relaxed);
    }

    // ── nr_running helpers ───────────────────────────────────────────────────

    pub fn inc_nr_running(&self) {
        self.nr_running.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_nr_running(&self) {
        self.nr_running.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn get_nr_running(&self) -> u32 {
        self.nr_running.load(Ordering::Relaxed)
    }

    // ── dispatch: pick_next_task ─────────────────────────────────────────────

    /// Pick the next task to run, dispatching through the class hierarchy.
    ///
    /// Order: RT (highest priority) → CFS → idle.
    ///
    /// Mirrors Linux `pick_next_task()` (core.c).
    pub fn pick_next_task(&mut self) -> Option<Pid> {
        use super::cfs::CFS_SCHED_CLASS;
        use super::rt::RT_SCHED_CLASS;
        use super::sched_class::SchedClass;

        // 1. Try the RT class first.
        if self.rt.nr_running > 0 {
            if let Some(pid) = RT_SCHED_CLASS.pick_next_task(self) {
                self.curr = Some(pid);
                self.nr_switches += 1;
                return Some(pid);
            }
        }

        // 2. Fall through to CFS.
        if self.cfs.nr_running > 0 {
            if let Some(pid) = CFS_SCHED_CLASS.pick_next_task(self) {
                self.curr = Some(pid);
                self.nr_switches += 1;
                return Some(pid);
            }
        }

        // 3. Idle.
        self.curr = self.idle;
        self.idle
    }

    // ── load weight for this CPU ─────────────────────────────────────────────

    /// Approximate load: number of runnable tasks × NICE_0_WEIGHT.
    pub fn load_weight(&self) -> u64 {
        self.get_nr_running() as u64 * super::cfs::NICE_0_WEIGHT
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-CPU run-queue registry
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum CPUs supported by the load balancer.
pub const MAX_CPUS: usize = 64;

/// The global array of per-CPU run queues.
///
/// In a real SMP kernel these would live in per-CPU memory; here we store them
/// in a static protected by a spin-lock per slot.
///
/// Note: initialised lazily; callers must call `init_run_queues()` before use.
static RUN_QUEUES: spin::Mutex<alloc::vec::Vec<RunQueue>> =
    spin::Mutex::new(alloc::vec::Vec::new());

/// Initialise `num_cpus` per-CPU run queues.
pub fn init_run_queues(num_cpus: u32) {
    let mut rqs = RUN_QUEUES.lock();
    rqs.clear();
    for cpu in 0..num_cpus {
        rqs.push(RunQueue::new(cpu));
    }
}

/// Execute a closure with mutable access to the run queue for `cpu`.
/// Returns `None` if `cpu` is out of range.
pub fn with_run_queue<F, R>(cpu: u32, f: F) -> Option<R>
where
    F: FnOnce(&mut RunQueue) -> R,
{
    let mut rqs = RUN_QUEUES.lock();
    let rq = rqs.get_mut(cpu as usize)?;
    Some(f(rq))
}

/// Read the nr_running for `cpu` without mutating the queue.
pub fn cpu_nr_running(cpu: u32) -> u32 {
    let rqs = RUN_QUEUES.lock();
    rqs.get(cpu as usize)
        .map(|rq| rq.get_nr_running())
        .unwrap_or(0)
}

/// Return the approximate load weight for `cpu`.
pub fn cpu_load(cpu: u32) -> u64 {
    let rqs = RUN_QUEUES.lock();
    rqs.get(cpu as usize)
        .map(|rq| rq.load_weight())
        .unwrap_or(0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Scheduling domain  (simplified single-level)
// ─────────────────────────────────────────────────────────────────────────────

/// Scheduling domain flags.
///
/// Mirrors Linux `SD_*` flags (sched/topology.c).
pub mod sd_flags {
    /// This domain spans physical-package (NUMA) boundaries
    pub const SD_NUMA: u32 = 1 << 0;
    /// Balance on fork (exec)
    pub const SD_BALANCE_EXEC: u32 = 1 << 1;
    /// Balance on wakeup
    pub const SD_BALANCE_WAKE: u32 = 1 << 2;
    /// Enable wake-idle-nearest-node heuristic
    pub const SD_WAKE_AFFINE: u32 = 1 << 3;
    /// Load balance when this CPU becomes idle
    pub const SD_BALANCE_NEWIDLE: u32 = 1 << 4;
}

/// A scheduling domain: a set of CPUs that can migrate tasks between each other.
///
/// Mirrors Linux `struct sched_domain` (simplified).
#[derive(Debug, Clone)]
pub struct SchedDomain {
    /// Bitmask of CPUs in this domain (one bit per CPU id)
    pub cpu_mask: u64,
    /// Flags (SD_* constants above)
    pub flags: u32,
    /// Minimum interval between load-balance calls on this domain (ns)
    pub min_interval_ns: u64,
    /// Maximum interval
    pub max_interval_ns: u64,
    /// Last time load balance ran on this domain (ns)
    pub last_balance_ns: u64,
    /// Imbalance threshold (tasks × NICE_0_WEIGHT)
    pub imbalance_pct: u32,
}

impl SchedDomain {
    /// Construct a flat SMP domain covering `num_cpus` CPUs.
    pub fn flat_smp(num_cpus: u32) -> Self {
        let cpu_mask = if num_cpus >= 64 {
            u64::MAX
        } else {
            (1u64 << num_cpus) - 1
        };
        Self {
            cpu_mask,
            flags: sd_flags::SD_BALANCE_EXEC
                | sd_flags::SD_BALANCE_WAKE
                | sd_flags::SD_WAKE_AFFINE
                | sd_flags::SD_BALANCE_NEWIDLE,
            min_interval_ns: 1_000_000,  // 1 ms
            max_interval_ns: 32_000_000, // 32 ms
            last_balance_ns: 0,
            imbalance_pct: 117, // 17 % imbalance threshold (Linux default)
        }
    }

    /// Return an iterator over CPU ids in this domain.
    pub fn cpus(&self) -> impl Iterator<Item = u32> {
        let mask = self.cpu_mask;
        (0u32..64).filter(move |&i| mask & (1u64 << i) != 0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LoadBalanceResult — bookkeeping returned from load_balance()
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct LoadBalanceResult {
    /// Number of tasks pulled to the local CPU
    pub tasks_moved: u32,
    /// Load of the busiest CPU before migration
    pub busiest_load: u64,
    /// Load of the local CPU before migration
    pub local_load: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// load_balance()
// ─────────────────────────────────────────────────────────────────────────────

/// Attempt to pull tasks from the busiest CPU in `domain` to `this_cpu`.
///
/// Mirrors Linux `load_balance()` (core.c).
///
/// Returns the number of tasks moved.
///
/// Algorithm:
///   1. Find the busiest CPU in the domain (highest load weight).
///   2. Compute the imbalance (busiest − local).
///   3. If imbalance > threshold, request migration of ≤ `nr_to_move` tasks.
///
/// The actual task migration is advisory here — the scheduler records how many
/// tasks *should* be moved; in a real kernel the per-CPU task lists would be
/// locked and entries physically migrated.  This implementation updates
/// `RunQueue::nr_running` counters to reflect the simulated migration.
pub fn load_balance(
    this_cpu: u32,
    domain: &mut SchedDomain,
    idle: CpuIdleType,
) -> LoadBalanceResult {
    let mut result = LoadBalanceResult::default();

    let local_load = cpu_load(this_cpu);
    result.local_load = local_load;

    // Find the busiest CPU in this domain (excluding this_cpu).
    let busiest_cpu = domain
        .cpus()
        .filter(|&c| c != this_cpu)
        .max_by_key(|&c| cpu_load(c));

    let busiest_cpu = match busiest_cpu {
        Some(c) => c,
        None => return result, // single-CPU domain
    };

    let busiest_load = cpu_load(busiest_cpu);
    result.busiest_load = busiest_load;

    if busiest_load == 0 {
        return result;
    }

    // Imbalance check: only migrate if busiest is significantly heavier.
    // Linux uses: avg_load > (local_load * imbalance_pct) / 100
    let threshold = local_load.saturating_mul(domain.imbalance_pct as u64) / 100;
    if busiest_load <= threshold {
        return result;
    }

    // Number of tasks to move: aim to equalise load.
    // Simplified: move at most half the imbalance / NICE_0_WEIGHT tasks.
    let imbalance = busiest_load.saturating_sub(local_load);
    let nr_to_move = (imbalance / (2 * super::cfs::NICE_0_WEIGHT)).max(1) as u32;

    // Simulate migration: decrement busiest, increment local.
    // (A real implementation would grab both run-queue locks and splice tasks.)
    let actual_moved = {
        let rqs = RUN_QUEUES.lock();
        let busiest_nr = rqs
            .get(busiest_cpu as usize)
            .map(|rq| rq.get_nr_running())
            .unwrap_or(0);
        let can_move = nr_to_move.min(busiest_nr / 2);
        can_move
    };

    if actual_moved > 0 {
        if let Some(rq) = RUN_QUEUES.lock().get(busiest_cpu as usize) {
            for _ in 0..actual_moved {
                rq.dec_nr_running();
            }
        }
        if let Some(rq) = RUN_QUEUES.lock().get(this_cpu as usize) {
            for _ in 0..actual_moved {
                rq.inc_nr_running();
            }
        }
        result.tasks_moved = actual_moved;
    }

    result
}

// ─────────────────────────────────────────────────────────────────────────────
// rebalance_domains()
// ─────────────────────────────────────────────────────────────────────────────

/// Walk all scheduling domains and trigger `load_balance` as needed.
///
/// Mirrors Linux `rebalance_domains()` (core.c).
///
/// In a production kernel there is a hierarchy of domains (SMT thread siblings
/// → core siblings → NUMA node siblings → …).  Here we maintain a single flat
/// SMP domain.
pub fn rebalance_domains(this_cpu: u32, idle: CpuIdleType, now_ns: u64, domain: &mut SchedDomain) {
    // Skip if we balanced too recently.
    if now_ns < domain.last_balance_ns + domain.min_interval_ns {
        return;
    }

    load_balance(this_cpu, domain, idle);

    // Back-off: increase the interval up to max (Linux uses exponential back-off
    // when no tasks are moved; we keep it simple here).
    domain.last_balance_ns = now_ns;
}

// ─────────────────────────────────────────────────────────────────────────────
// trigger_load_balance()
// ─────────────────────────────────────────────────────────────────────────────

/// Entry point called from the per-CPU timer tick.
///
/// Mirrors Linux `trigger_load_balance()` (core.c).  Raises `SCHED_SOFTIRQ`
/// so that `rebalance_domains` runs in a deferred soft-IRQ context rather
/// than inline in the timer tick.
pub fn trigger_load_balance(
    this_cpu: u32,
    idle: CpuIdleType,
    now_ns: u64,
    domain: &mut SchedDomain,
) {
    // Store parameters for the softirq handler to pick up.
    LB_PARAMS.lock().get_or_insert_with(|| LbParams {
        cpu: this_cpu,
        idle,
        now_ns,
    });
    crate::softirq::raise_softirq(crate::softirq::SCHED_SOFTIRQ);
}

/// Parameters for the deferred load-balance softirq handler.
struct LbParams {
    cpu: u32,
    idle: CpuIdleType,
    now_ns: u64,
}

static LB_PARAMS: spin::Mutex<Option<LbParams>> = spin::Mutex::new(None);

/// Softirq handler for `SCHED_SOFTIRQ` — runs `rebalance_domains` in a
/// deferred context.  Registered during scheduler init.
pub fn sched_softirq_action() {
    let params = LB_PARAMS.lock().take();
    if let Some(p) = params {
        let num_cpus = crate::smp::cpu_count();
        let mut domain = SchedDomain::flat_smp(num_cpus);
        rebalance_domains(p.cpu, p.idle, p.now_ns, &mut domain);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Idle class helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Select the most idle CPU for a new task.
///
/// Returns the CPU id with the fewest runnable tasks, or `preferred` if all
/// CPUs are equally loaded.
///
/// Mirrors the core idea in Linux `select_idle_cpu()` (fair.c).
pub fn select_idle_cpu(preferred: u32, domain: &SchedDomain) -> u32 {
    let mut best_cpu = preferred;
    let mut best_nr = cpu_nr_running(preferred);

    for cpu in domain.cpus() {
        if cpu == preferred {
            continue;
        }
        let nr = cpu_nr_running(cpu);
        if nr < best_nr {
            best_nr = nr;
            best_cpu = cpu;
        }
    }

    best_cpu
}

/// Return true if `cpu` is currently idle (no runnable tasks).
pub fn cpu_is_idle(cpu: u32) -> bool {
    cpu_nr_running(cpu) == 0
}
