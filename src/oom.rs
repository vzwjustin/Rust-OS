//! OOM Killer — Out-of-Memory process killer
//!
//! Ported from Linux mm/oom_kill.c.
//! When memory allocation fails and reclaim can't free enough pages,
//! the OOM killer selects the "worst" process (by memory usage heuristic)
//! and terminates it to free memory.
//!
//! ## Heuristic (from Linux oom_badness)
//! Points = rss + swap_entries + page_tables / PAGE_SIZE + oom_score_adj
//! The process with the highest points is killed.

use alloc::string::ToString;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

// ── OOM score adjustment constants ──────────────────────────────────────

pub const OOM_SCORE_ADJ_MIN: i32 = -1000;
pub const OOM_SCORE_ADJ_MAX: i32 = 1000;
pub const OOM_SCORE_ADJ_DEFAULT: i32 = 0;

// ── Sysctl-like tunables ────────────────────────────────────────────────

static PANIC_ON_OOM: AtomicBool = AtomicBool::new(false);
static OOM_KILL_ALLOCATING_TASK: AtomicBool = AtomicBool::new(false);
static OOM_DUMP_TASKS: AtomicBool = AtomicBool::new(true);
static OOM_KILL_COUNT: AtomicU64 = AtomicU64::new(0);
static OOM_DISABLED: AtomicBool = AtomicBool::new(false);

// ── OOM control structure ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OomControl {
    pub totalpages: u64,
    pub killed: bool,
}

impl OomControl {
    pub fn new() -> Self {
        Self {
            totalpages: 0,
            killed: false,
        }
    }
}

// ── Per-process OOM data ────────────────────────────────────────────────

/// OOM-related data for a process, stored in the PCB.
#[derive(Debug, Clone, Copy)]
pub struct OomProcessData {
    pub oom_score_adj: i32,
    pub oom_score_adj_min: i32,
}

impl Default for OomProcessData {
    fn default() -> Self {
        Self {
            oom_score_adj: OOM_SCORE_ADJ_DEFAULT,
            oom_score_adj_min: OOM_SCORE_ADJ_MIN,
        }
    }
}

// ── Process scoring ─────────────────────────────────────────────────────

/// Information about a process for OOM scoring.
#[derive(Debug, Clone)]
pub struct OomCandidate {
    pub pid: u32,
    pub name: alloc::string::String,
    pub rss_pages: u64,
    pub swap_entries: u64,
    pub page_table_pages: u64,
    pub oom_score_adj: i32,
    pub is_kernel_thread: bool,
    pub is_init: bool,
}

impl OomCandidate {
    /// Calculate the OOM badness score (higher = more likely to be killed).
    /// Ported from Linux oom_badness().
    pub fn badness(&self, totalpages: u64) -> i64 {
        // Don't kill init or kernel threads
        if self.is_init || self.is_kernel_thread {
            return i64::MIN;
        }

        // Don't kill processes with OOM_SCORE_ADJ_MIN
        if self.oom_score_adj == OOM_SCORE_ADJ_MIN {
            return i64::MIN;
        }

        // Baseline: proportion of RAM used by this process
        let mut points: i64 = (self.rss_pages + self.swap_entries + self.page_table_pages) as i64;

        // Normalize by oom_score_adj
        // adj *= totalpages / 1000
        let adj = self.oom_score_adj as i64 * (totalpages as i64 / 1000);
        points += adj;

        points
    }
}

// ── OOM killer core ─────────────────────────────────────────────────────

static OOM_LOCK: Mutex<()> = Mutex::new(());

/// Collect all OOM candidates from the process manager.
fn collect_candidates() -> Vec<OomCandidate> {
    let pm = crate::process::get_process_manager();
    let pcbs = pm.find_processes(|_| true);
    let mut candidates = Vec::new();

    for pcb in pcbs {
        // Estimate RSS from VM size (simplified — real impl would count mapped pages)
        let rss_pages = (pcb.memory.vm_size / 4096).max(pcb.memory.heap_size / 4096);
        candidates.push(OomCandidate {
            pid: pcb.pid,
            name: pcb.name_str().to_string(),
            rss_pages,
            swap_entries: 0,     // No swap tracking per-process yet
            page_table_pages: 1, // Approximate
            oom_score_adj: OOM_SCORE_ADJ_DEFAULT,
            is_kernel_thread: pcb.pid > 1 && pcb.memory.vm_start == 0, // Heuristic
            is_init: pcb.pid == 1,
        });
    }

    candidates
}

/// Select the best process to kill. Returns the PID of the victim.
/// Ported from Linux select_bad_process().
pub fn select_bad_process(oc: &mut OomControl) -> Option<u32> {
    let candidates = collect_candidates();
    let totalpages = if oc.totalpages > 0 {
        oc.totalpages
    } else {
        // Estimate total pages from memory stats
        crate::memory::get_memory_stats()
            .map(|s| (s.total_memory / 4096) as u64)
            .unwrap_or(0)
    };

    let mut best_pid: Option<u32> = None;
    let mut best_points = i64::MIN;

    for candidate in &candidates {
        let points = candidate.badness(totalpages);
        if points > best_points {
            best_points = points;
            best_pid = Some(candidate.pid);
        }
    }

    if best_points == i64::MIN {
        None
    } else {
        best_pid
    }
}

/// Kill a process by PID. Sends SIGKILL (signal 9).
fn oom_kill_process(pid: u32, reason: &str) {
    crate::serial_println!(
        "[oom] killing pid {} ({}) — {}",
        pid,
        reason,
        "out of memory"
    );

    OOM_KILL_COUNT.fetch_add(1, Ordering::Relaxed);

    // Terminate via the process manager
    let pm = crate::process::get_process_manager();
    let _ = pm.terminate_process(pid, 9); // SIGKILL exit status

    // Also terminate all threads in the process
    let _ = pm.terminate_process_threads(pid);
}

/// Main OOM killer entry point. Called when allocation fails.
/// Ported from Linux out_of_memory().
pub fn out_of_memory(oc: &mut OomControl) -> bool {
    if OOM_DISABLED.load(Ordering::Acquire) {
        return false;
    }

    let _lock = OOM_LOCK.lock();

    // Optionally kill the allocating task directly
    if OOM_KILL_ALLOCATING_TASK.load(Ordering::Acquire) {
        let current_pid = crate::process::current_pid();
        if current_pid > 1 {
            oom_kill_process(current_pid, "allocating task");
            oc.killed = true;
            return true;
        }
    }

    // Select the worst process
    if let Some(victim_pid) = select_bad_process(oc) {
        // Dump task list if enabled
        if OOM_DUMP_TASKS.load(Ordering::Acquire) {
            dump_oom_candidates(oc);
        }

        oom_kill_process(victim_pid, "worst badness score");
        oc.killed = true;
        return true;
    }

    // No killable process found
    if PANIC_ON_OOM.load(Ordering::Acquire) {
        crate::serial_println!("[oom] no killable process, panicking");
        panic!("Out of memory: no killable process found");
    }

    crate::serial_println!("[oom] no killable process found");
    false
}

/// Dump all OOM candidates for debugging.
fn dump_oom_candidates(oc: &OomControl) {
    let candidates = collect_candidates();
    let totalpages = if oc.totalpages > 0 {
        oc.totalpages
    } else {
        crate::memory::get_memory_stats()
            .map(|s| (s.total_memory / 4096) as u64)
            .unwrap_or(0)
    };

    crate::serial_println!("[oom] === OOM candidate dump ===");
    for c in &candidates {
        let points = c.badness(totalpages);
        crate::serial_println!(
            "[oom] pid={} name={} rss={} swap={} adj={} points={}",
            c.pid,
            c.name,
            c.rss_pages,
            c.swap_entries,
            c.oom_score_adj,
            points
        );
    }
    crate::serial_println!("[oom] === end dump ===");
}

// ── Sysctl API ──────────────────────────────────────────────────────────

pub fn set_panic_on_oom(val: bool) {
    PANIC_ON_OOM.store(val, Ordering::Release);
}

pub fn set_oom_kill_allocating_task(val: bool) {
    OOM_KILL_ALLOCATING_TASK.store(val, Ordering::Release);
}

pub fn set_oom_dump_tasks(val: bool) {
    OOM_DUMP_TASKS.store(val, Ordering::Release);
}

pub fn set_oom_disabled(val: bool) {
    OOM_DISABLED.store(val, Ordering::Release);
}

pub fn oom_kill_count() -> u64 {
    OOM_KILL_COUNT.load(Ordering::Relaxed)
}

// ── Initialization ──────────────────────────────────────────────────────

pub fn init() {
    crate::serial_println!("[oom] OOM killer initialized");
}

// ── Memory pressure check ───────────────────────────────────────────────

/// Check if the system is under memory pressure and trigger OOM if needed.
/// Called from the memory allocator when allocation fails.
pub fn check_memory_pressure() -> bool {
    if let Some(stats) = crate::memory::get_memory_stats() {
        let free_pages = (stats.free_memory / 4096) as u64;
        let total_pages = (stats.total_memory / 4096) as u64;

        // If free memory is below 1% of total, trigger OOM
        if total_pages > 0 && free_pages * 100 < total_pages {
            let mut oc = OomControl::new();
            oc.totalpages = total_pages;
            return out_of_memory(&mut oc);
        }
    }

    false
}
