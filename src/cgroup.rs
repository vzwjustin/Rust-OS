//! Cgroups — Control Groups for resource management
//!
//! Ported from Linux kernel/cgroup/cgroup.c.
//! Provides hierarchical grouping of processes for resource control:
//! - Process grouping and hierarchy
//! - Resource controllers (subsystems): memory, cpu, pids, blkio
//! - Per-cgroup resource limits and accounting
//! - Process attachment/detachment
//!
//! ## Supported controllers
//! - memory: max memory usage, OOM control
//! - cpu: CPU time accounting and throttling
//! - pids: max process count
//! - blkio: I/O bandwidth tracking

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

// ── Controller IDs ──────────────────────────────────────────────────────

pub const CGROUP_MEMORY: usize = 0;
pub const CGROUP_CPU: usize = 1;
pub const CGROUP_PIDS: usize = 2;
pub const CGROUP_BLKIO: usize = 3;
pub const NR_CGROUP_SUBSYS: usize = 4;

pub static CGROUP_SUBSYS_NAMES: [&str; NR_CGROUP_SUBSYS] = ["memory", "cpu", "pids", "blkio"];

// ── Cgroup ──────────────────────────────────────────────────────────────

/// A cgroup is a group of processes that share resource limits.
pub struct Cgroup {
    pub id: u32,
    pub name: String,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub processes: Vec<u32>, // PIDs
    pub controllers: CgroupControllers,
    pub level: u32,
}

/// Per-controller state for a cgroup.
#[derive(Debug, Clone)]
pub struct CgroupControllers {
    pub memory: MemoryController,
    pub cpu: CpuController,
    pub pids: PidsController,
    pub blkio: BlkioController,
}

impl Default for CgroupControllers {
    fn default() -> Self {
        Self {
            memory: MemoryController::default(),
            cpu: CpuController::default(),
            pids: PidsController::default(),
            blkio: BlkioController::default(),
        }
    }
}

// ── Memory controller ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MemoryController {
    pub max_bytes: u64, // 0 = unlimited
    pub current_bytes: u64,
    pub swap_max_bytes: u64, // 0 = unlimited
    pub swap_current: u64,
    pub oom_kill_disable: bool,
    pub oom_score_adj: i32,
    pub page_faults: u64,
    pub oom_count: u64,
}

impl Default for MemoryController {
    fn default() -> Self {
        Self {
            max_bytes: 0,
            current_bytes: 0,
            swap_max_bytes: 0,
            swap_current: 0,
            oom_kill_disable: false,
            oom_score_adj: 0,
            page_faults: 0,
            oom_count: 0,
        }
    }
}

impl MemoryController {
    /// Check if allocating `bytes` would exceed the limit.
    pub fn check_limit(&self, bytes: u64) -> bool {
        if self.max_bytes == 0 {
            return true; // Unlimited
        }
        self.current_bytes + bytes <= self.max_bytes
    }

    /// Charge memory usage to this cgroup.
    pub fn charge(&mut self, bytes: u64) -> bool {
        if !self.check_limit(bytes) {
            return false;
        }
        self.current_bytes += bytes;
        true
    }

    /// Uncharge memory usage from this cgroup.
    pub fn uncharge(&mut self, bytes: u64) {
        self.current_bytes = self.current_bytes.saturating_sub(bytes);
    }
}

// ── CPU controller ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CpuController {
    pub cpu_time_ns: u64,   // Total CPU time used (all time)
    pub cpu_quota_us: i64,  // -1 = unlimited, else max CPU time per period
    pub cpu_period_us: u64, // Period for quota (default 100000 = 100ms)
    pub cpu_shares: u64,    // Weight for proportional scheduling (default 1024)
    pub nr_throttled: u64,  // Times throttled
    pub throttled_time_ns: u64,
    /// Monotonic ns timestamp when the current period started (0 = unset)
    pub period_start_ns: u64,
    /// CPU nanoseconds consumed within the current period
    pub used_ns_in_period: u64,
}

impl Default for CpuController {
    fn default() -> Self {
        Self {
            cpu_time_ns: 0,
            cpu_quota_us: -1,
            cpu_period_us: 100_000,
            cpu_shares: 1024,
            nr_throttled: 0,
            throttled_time_ns: 0,
            period_start_ns: 0,
            used_ns_in_period: 0,
        }
    }
}

impl CpuController {
    /// Charge CPU time to this cgroup.
    pub fn charge_cpu(&mut self, ns: u64) {
        self.cpu_time_ns += ns;
        self.used_ns_in_period += ns;
    }

    /// Reset the current period if `current_time_ns` has crossed the period boundary.
    pub fn maybe_reset_period(&mut self, current_time_ns: u64) {
        let period_ns = self.cpu_period_us * 1000;
        if self.period_start_ns == 0 {
            self.period_start_ns = current_time_ns;
            self.used_ns_in_period = 0;
            return;
        }
        if current_time_ns >= self.period_start_ns + period_ns {
            self.used_ns_in_period = 0;
            // Advance period_start by the number of complete periods that elapsed
            let elapsed = current_time_ns - self.period_start_ns;
            let periods = elapsed / period_ns;
            self.period_start_ns += periods * period_ns;
        }
    }

    /// Check if the cgroup has exceeded its CPU quota in the current period.
    pub fn is_throttled(&self) -> bool {
        if self.cpu_quota_us < 0 {
            return false;
        }
        let quota_ns = self.cpu_quota_us as u64 * 1000;
        self.used_ns_in_period >= quota_ns
    }
}

// ── PIDs controller ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PidsController {
    pub max: i64, // -1 = unlimited
    pub current: i64,
}

impl Default for PidsController {
    fn default() -> Self {
        Self {
            max: -1,
            current: 0,
        }
    }
}

impl PidsController {
    pub fn can_fork(&self) -> bool {
        if self.max < 0 {
            return true;
        }
        self.current < self.max
    }

    pub fn fork_charge(&mut self) -> bool {
        if !self.can_fork() {
            return false;
        }
        self.current += 1;
        true
    }

    pub fn fork_uncharge(&mut self) {
        self.current -= 1;
    }
}

// ── Blkio controller ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BlkioController {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_ios: u64,
    pub write_ios: u64,
    pub throttle_read_bps: u64,  // 0 = unlimited
    pub throttle_write_bps: u64, // 0 = unlimited
}

impl Default for BlkioController {
    fn default() -> Self {
        Self {
            read_bytes: 0,
            write_bytes: 0,
            read_ios: 0,
            write_ios: 0,
            throttle_read_bps: 0,
            throttle_write_bps: 0,
        }
    }
}

impl BlkioController {
    pub fn charge_read(&mut self, bytes: u64) {
        self.read_bytes += bytes;
        self.read_ios += 1;
    }

    pub fn charge_write(&mut self, bytes: u64) {
        self.write_bytes += bytes;
        self.write_ios += 1;
    }
}

// ── Global cgroup state ─────────────────────────────────────────────────

static CGROUPS: RwLock<BTreeMap<u32, Cgroup>> = RwLock::new(BTreeMap::new());
static NEXT_CGROUP_ID: AtomicU64 = AtomicU64::new(1);
static ROOT_CGROUP_ID: u32 = 1;

/// Map from PID → cgroup ID
static PID_TO_CGROUP: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

// ── Initialization ──────────────────────────────────────────────────────

/// Early cgroup initialization — creates the root cgroup so that the
/// scheduler and process creation can assign PIDs to a cgroup from the
/// very beginning.  Mirrors Linux's `cgroup_init_early()` in start_kernel().
pub fn init_early() {
    let root = Cgroup {
        id: ROOT_CGROUP_ID,
        name: String::from("/"),
        parent: None,
        children: Vec::new(),
        processes: Vec::new(),
        controllers: CgroupControllers::default(),
        level: 0,
    };
    CGROUPS.write().insert(ROOT_CGROUP_ID, root);
}

pub fn init() {
    // Create the root cgroup if early init didn't run
    if CGROUPS.read().get(&ROOT_CGROUP_ID).is_none() {
        init_early();
    }
    crate::serial_println!("[cgroup] initialized (root cgroup id={})", ROOT_CGROUP_ID);
}

// ── Cgroup creation/destruction ─────────────────────────────────────────

/// Create a new cgroup under the given parent.
pub fn create_cgroup(parent_id: u32, name: &str) -> Result<u32, &'static str> {
    let mut groups = CGROUPS.write();

    // Verify parent exists
    let parent_level = groups
        .get(&parent_id)
        .map(|p| p.level)
        .ok_or("parent cgroup not found")?;

    let id = NEXT_CGROUP_ID.fetch_add(1, Ordering::SeqCst) as u32;

    let cgroup = Cgroup {
        id,
        name: String::from(name),
        parent: Some(parent_id),
        children: Vec::new(),
        processes: Vec::new(),
        controllers: CgroupControllers::default(),
        level: parent_level + 1,
    };

    // Add to parent's children list
    if let Some(parent) = groups.get_mut(&parent_id) {
        parent.children.push(id);
    }

    groups.insert(id, cgroup);

    crate::serial_println!(
        "[cgroup] created '{}' (id={}, parent={})",
        name,
        id,
        parent_id
    );
    Ok(id)
}

/// Destroy a cgroup (must be empty).
pub fn destroy_cgroup(id: u32) -> Result<(), &'static str> {
    let mut groups = CGROUPS.write();

    if id == ROOT_CGROUP_ID {
        return Err("cannot destroy root cgroup");
    }

    let cgroup = groups.get(&id).ok_or("cgroup not found")?;

    if !cgroup.processes.is_empty() {
        return Err("cgroup not empty");
    }

    if !cgroup.children.is_empty() {
        return Err("cgroup has children");
    }

    // Remove from parent's children list
    if let Some(parent_id) = cgroup.parent {
        if let Some(parent) = groups.get_mut(&parent_id) {
            parent.children.retain(|&c| c != id);
        }
    }

    groups.remove(&id);
    crate::serial_println!("[cgroup] destroyed id={}", id);
    Ok(())
}

// ── Process attachment ──────────────────────────────────────────────────

/// Attach a process to a cgroup.
pub fn attach_process(cgroup_id: u32, pid: u32) -> Result<(), &'static str> {
    let mut groups = CGROUPS.write();

    // Verify cgroup exists
    if !groups.contains_key(&cgroup_id) {
        return Err("cgroup not found");
    }

    // Remove from old cgroup
    let old_cgroup = PID_TO_CGROUP.read().get(&pid).copied();
    if let Some(old_id) = old_cgroup {
        if let Some(old) = groups.get_mut(&old_id) {
            old.processes.retain(|&p| p != pid);
        }
    }

    // Add to new cgroup
    if let Some(cg) = groups.get_mut(&cgroup_id) {
        cg.processes.push(pid);
    }

    drop(groups);
    PID_TO_CGROUP.write().insert(pid, cgroup_id);

    Ok(())
}

/// Get the cgroup ID for a given PID.
pub fn get_cgroup_for_pid(pid: u32) -> u32 {
    PID_TO_CGROUP
        .read()
        .get(&pid)
        .copied()
        .unwrap_or(ROOT_CGROUP_ID)
}

/// Get all processes in a cgroup.
pub fn cgroup_processes(cgroup_id: u32) -> Vec<u32> {
    CGROUPS
        .read()
        .get(&cgroup_id)
        .map(|cg| cg.processes.clone())
        .unwrap_or_default()
}

// ── Controller operations ───────────────────────────────────────────────

/// Set memory limit for a cgroup.
pub fn set_memory_limit(cgroup_id: u32, max_bytes: u64) -> Result<(), &'static str> {
    let mut groups = CGROUPS.write();
    let cg = groups.get_mut(&cgroup_id).ok_or("cgroup not found")?;
    cg.controllers.memory.max_bytes = max_bytes;
    Ok(())
}

/// Set CPU quota for a cgroup.
pub fn set_cpu_quota(cgroup_id: u32, quota_us: i64, period_us: u64) -> Result<(), &'static str> {
    let mut groups = CGROUPS.write();
    let cg = groups.get_mut(&cgroup_id).ok_or("cgroup not found")?;
    cg.controllers.cpu.cpu_quota_us = quota_us;
    cg.controllers.cpu.cpu_period_us = period_us;
    Ok(())
}

/// Set CPU shares (weight) for a cgroup.
pub fn set_cpu_shares(cgroup_id: u32, shares: u64) -> Result<(), &'static str> {
    let mut groups = CGROUPS.write();
    let cg = groups.get_mut(&cgroup_id).ok_or("cgroup not found")?;
    cg.controllers.cpu.cpu_shares = shares;
    Ok(())
}

/// Set max PIDs for a cgroup.
pub fn set_pids_max(cgroup_id: u32, max: i64) -> Result<(), &'static str> {
    let mut groups = CGROUPS.write();
    let cg = groups.get_mut(&cgroup_id).ok_or("cgroup not found")?;
    cg.controllers.pids.max = max;
    Ok(())
}

/// Set blkio read throttle (bytes per second).
pub fn set_blkio_read_bps(cgroup_id: u32, bps: u64) -> Result<(), &'static str> {
    let mut groups = CGROUPS.write();
    let cg = groups.get_mut(&cgroup_id).ok_or("cgroup not found")?;
    cg.controllers.blkio.throttle_read_bps = bps;
    Ok(())
}

/// Set blkio write throttle (bytes per second).
pub fn set_blkio_write_bps(cgroup_id: u32, bps: u64) -> Result<(), &'static str> {
    let mut groups = CGROUPS.write();
    let cg = groups.get_mut(&cgroup_id).ok_or("cgroup not found")?;
    cg.controllers.blkio.throttle_write_bps = bps;
    Ok(())
}

// ── Resource charging ───────────────────────────────────────────────────

/// Charge memory usage to the cgroup that owns the given PID.
pub fn charge_memory(pid: u32, bytes: u64) -> bool {
    let cgroup_id = get_cgroup_for_pid(pid);
    let mut groups = CGROUPS.write();
    if let Some(cg) = groups.get_mut(&cgroup_id) {
        return cg.controllers.memory.charge(bytes);
    }
    true // No cgroup = unlimited
}

/// Uncharge memory usage from the cgroup that owns the given PID.
pub fn uncharge_memory(pid: u32, bytes: u64) {
    let cgroup_id = get_cgroup_for_pid(pid);
    let mut groups = CGROUPS.write();
    if let Some(cg) = groups.get_mut(&cgroup_id) {
        cg.controllers.memory.uncharge(bytes);
    }
}

/// Charge CPU time to the cgroup that owns the given PID.
pub fn charge_cpu_time(pid: u32, ns: u64) {
    let cgroup_id = get_cgroup_for_pid(pid);
    let mut groups = CGROUPS.write();
    if let Some(cg) = groups.get_mut(&cgroup_id) {
        cg.controllers.cpu.charge_cpu(ns);
    }
}

/// Check if a fork is allowed under the PID controller.
pub fn can_fork(pid: u32) -> bool {
    let cgroup_id = get_cgroup_for_pid(pid);
    let groups = CGROUPS.read();
    if let Some(cg) = groups.get(&cgroup_id) {
        return cg.controllers.pids.can_fork();
    }
    true
}

/// Charge a new process to the PID controller.
pub fn fork_charge(pid: u32, new_pid: u32) -> bool {
    let cgroup_id = get_cgroup_for_pid(pid);
    let mut groups = CGROUPS.write();
    if let Some(cg) = groups.get_mut(&cgroup_id) {
        if !cg.controllers.pids.fork_charge() {
            return false;
        }
        if !cg.processes.contains(&new_pid) {
            cg.processes.push(new_pid);
        }
    }
    // New process inherits parent's cgroup
    drop(groups);
    PID_TO_CGROUP.write().insert(new_pid, cgroup_id);
    true
}

/// Uncharge a process from the PID controller (on exit).
pub fn fork_uncharge(pid: u32) {
    let cgroup_id = get_cgroup_for_pid(pid);
    let mut groups = CGROUPS.write();
    if let Some(cg) = groups.get_mut(&cgroup_id) {
        cg.controllers.pids.fork_uncharge();
        cg.processes.retain(|&p| p != pid);
    }
    drop(groups);
    PID_TO_CGROUP.write().remove(&pid);
}

/// Charge block I/O read to the cgroup that owns the given PID.
pub fn charge_blkio_read(pid: u32, bytes: u64) {
    let cgroup_id = get_cgroup_for_pid(pid);
    let mut groups = CGROUPS.write();
    if let Some(cg) = groups.get_mut(&cgroup_id) {
        cg.controllers.blkio.charge_read(bytes);
    }
}

/// Charge block I/O write to the cgroup that owns the given PID.
pub fn charge_blkio_write(pid: u32, bytes: u64) {
    let cgroup_id = get_cgroup_for_pid(pid);
    let mut groups = CGROUPS.write();
    if let Some(cg) = groups.get_mut(&cgroup_id) {
        cg.controllers.blkio.charge_write(bytes);
    }
}

// ── Hierarchy traversal ─────────────────────────────────────────────────

/// Get all cgroups in the hierarchy (depth-first).
pub fn list_cgroups() -> Vec<(u32, String, u32, usize)> {
    let groups = CGROUPS.read();
    groups
        .iter()
        .map(|(&id, cg)| (id, cg.name.clone(), cg.level, cg.processes.len()))
        .collect()
}

/// Get cgroup info by ID.
pub fn get_cgroup_info(cgroup_id: u32) -> Option<CgroupInfo> {
    let groups = CGROUPS.read();
    groups.get(&cgroup_id).map(|cg| CgroupInfo {
        id: cg.id,
        name: cg.name.clone(),
        level: cg.level,
        parent: cg.parent,
        children: cg.children.clone(),
        process_count: cg.processes.len(),
        memory_max: cg.controllers.memory.max_bytes,
        memory_current: cg.controllers.memory.current_bytes,
        cpu_time_ns: cg.controllers.cpu.cpu_time_ns,
        cpu_shares: cg.controllers.cpu.cpu_shares,
        pids_max: cg.controllers.pids.max,
        pids_current: cg.controllers.pids.current,
    })
}

#[derive(Debug, Clone)]
pub struct CgroupInfo {
    pub id: u32,
    pub name: String,
    pub level: u32,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub process_count: usize,
    pub memory_max: u64,
    pub memory_current: u64,
    pub cpu_time_ns: u64,
    pub cpu_shares: u64,
    pub pids_max: i64,
    pub pids_current: i64,
}

// ── OOM integration ─────────────────────────────────────────────────────

/// Check if a cgroup's memory limit is exceeded and trigger OOM if needed.
pub fn check_cgroup_oom(cgroup_id: u32) -> bool {
    let groups = CGROUPS.read();
    let Some(cg) = groups.get(&cgroup_id) else {
        return false;
    };

    if cg.controllers.memory.max_bytes == 0 {
        return false; // No limit
    }

    if cg.controllers.memory.current_bytes <= cg.controllers.memory.max_bytes {
        return false;
    }

    if cg.controllers.memory.oom_kill_disable {
        return false;
    }

    drop(groups);

    // Find the process with the most memory in this cgroup and kill it
    let procs = cgroup_processes(cgroup_id);
    if procs.is_empty() {
        return false;
    }

    // Simplified: kill the first process (real implementation would score them)
    crate::serial_println!(
        "[cgroup] OOM: killing pid {} in cgroup {} (memory limit exceeded)",
        procs[0],
        cgroup_id
    );

    let pm = crate::process::get_process_manager();
    let _ = pm.terminate_process(procs[0], 9);

    let mut groups = CGROUPS.write();
    if let Some(cg) = groups.get_mut(&cgroup_id) {
        cg.controllers.memory.oom_count += 1;
    }

    true
}

// ── Enforcement hooks (called from allocator / scheduler / I/O path) ─────

/// Check and charge a memory allocation for the **current** process.
///
/// Returns `true` if the allocation is within limits (and charges it),
/// `false` if the cgroup memory limit would be exceeded.
/// Call `uncharge_memory(current_pid, size)` if the allocation is later freed.
pub fn enforce_memory_limit(size: usize) -> bool {
    let pid = crate::process::current_pid();
    charge_memory(pid, size as u64)
}

/// Check whether a process is within its CPU quota for the current period.
///
/// Returns `true` if the process may be scheduled, `false` if it should be
/// throttled.  The scheduler should call this before dispatching a process.
pub fn check_cpu_quota(pid: u32) -> bool {
    let cgroup_id = get_cgroup_for_pid(pid);
    let groups = CGROUPS.read();
    if let Some(cg) = groups.get(&cgroup_id) {
        return !cg.controllers.cpu.is_throttled();
    }
    true // no cgroup = unlimited
}

/// Record CPU usage for a process (in microseconds) and handle period resets.
///
/// `microseconds` is the amount of CPU time consumed in this timeslice.
/// Call this from the scheduler tick / context-switch path.
pub fn record_cpu_usage(pid: u32, microseconds: u64) {
    let cgroup_id = get_cgroup_for_pid(pid);
    let mut groups = CGROUPS.write();
    if let Some(cg) = groups.get_mut(&cgroup_id) {
        let cpu = &mut cg.controllers.cpu;
        // Use cumulative cpu_time_ns as a monotonic clock proxy
        let clock_ns = cpu.cpu_time_ns;
        cpu.maybe_reset_period(clock_ns);
        cpu.charge_cpu(microseconds * 1000);
        if cpu.is_throttled() {
            cpu.nr_throttled += 1;
        }
    }
}

/// Check whether a block-I/O read of `bytes` is within the cgroup's throttle
/// limit for the process.  Returns `true` if allowed, `false` to rate-limit.
pub fn check_blkio_read(pid: u32, bytes: u64) -> bool {
    let cgroup_id = get_cgroup_for_pid(pid);
    let groups = CGROUPS.read();
    if let Some(cg) = groups.get(&cgroup_id) {
        let limit = cg.controllers.blkio.throttle_read_bps;
        if limit == 0 {
            return true; // unlimited
        }
        return bytes <= limit;
    }
    true
}

/// Check whether a block-I/O write of `bytes` is within the cgroup's throttle
/// limit.  Returns `true` if allowed, `false` to rate-limit.
pub fn check_blkio_write(pid: u32, bytes: u64) -> bool {
    let cgroup_id = get_cgroup_for_pid(pid);
    let groups = CGROUPS.read();
    if let Some(cg) = groups.get(&cgroup_id) {
        let limit = cg.controllers.blkio.throttle_write_bps;
        if limit == 0 {
            return true; // unlimited
        }
        return bytes <= limit;
    }
    true
}
