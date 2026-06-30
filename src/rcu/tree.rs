//! Tree RCU grace-period state machine.
//!
//! Ported (simplified) from Linux `kernel/rcu/tree.c`. Upstream tree RCU
//! drives a dedicated `rcu_sched` kthread through the states
//! `rcu_gp_init()` -> repeated `rcu_gp_fqs()` force-quiescent-state sweeps
//! -> `rcu_gp_cleanup()`, advancing `rcu_state.gp_seq` at the start and end
//! of every grace period. RustOS has no RCU kthread yet, so the same state
//! machine is instead driven cooperatively: one step runs every time the
//! RCU softirq fires (see `super::update::process_callbacks`) or when
//! `synchronize_rcu()` spins waiting for completion.
//!
//! `gp_seq` keeps Linux's parity convention: the low bit set means a grace
//! period is in progress (odd), clear means idle/completed (even). Each
//! full grace period therefore advances `gp_seq` by exactly 2: +1 at
//! `rcu_gp_init`, +1 at `rcu_gp_cleanup`.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

use crate::smp::MAX_CPUS;

/// Grace-period state machine states — analog of `rcu_state.gp_state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum GpState {
    /// No grace period in progress; waiting for a request.
    Idle = 0,
    /// `rcu_gp_init()` — grace period just started, snapshot taken.
    Init = 1,
    /// `rcu_gp_fqs()` — sweeping CPUs for quiescent states.
    FqsSweep = 2,
    /// `rcu_gp_cleanup()` — all CPUs quiesced, finalize the grace period.
    Cleanup = 3,
}

impl GpState {
    const fn from_u32(v: u32) -> GpState {
        match v {
            0 => GpState::Idle,
            1 => GpState::Init,
            2 => GpState::FqsSweep,
            _ => GpState::Cleanup,
        }
    }
}

/// Global RCU state — analog of Linux's `struct rcu_state rcu_state`.
pub struct RcuState {
    /// Grace-period sequence counter (`rcu_state.gp_seq`). Odd = in
    /// progress, even = idle/completed.
    pub gp_seq: AtomicU64,
    /// `gp_seq` value latched when the current grace period began
    /// (`rcu_state.gp_seq` as observed by `rcu_gp_init`).
    gp_seq_start: AtomicU64,
    /// Current state-machine state.
    gp_state: AtomicU32,
    /// Number of completed grace periods (`rcu_state.gp_seq >> shift`
    /// analog, kept as a plain counter here).
    gp_count: AtomicU64,
    /// Force-quiescent-state sweep counter (`rcu_state.n_force_qs`).
    n_force_qs: AtomicU64,
    /// Set when a caller (`call_rcu`/`synchronize_rcu`) needs a new grace
    /// period started; consumed by `rcu_gp_init`.
    gp_requested: AtomicBool,
}

impl RcuState {
    const fn new() -> Self {
        Self {
            gp_seq: AtomicU64::new(0),
            gp_seq_start: AtomicU64::new(0),
            gp_state: AtomicU32::new(GpState::Idle as u32),
            gp_count: AtomicU64::new(0),
            n_force_qs: AtomicU64::new(0),
            gp_requested: AtomicBool::new(false),
        }
    }

    fn state(&self) -> GpState {
        GpState::from_u32(self.gp_state.load(Ordering::Acquire))
    }

    fn set_state(&self, s: GpState) {
        self.gp_state.store(s as u32, Ordering::Release);
    }
}

/// Global RCU grace-period state (single instance — this kernel has one
/// flat RCU domain, unlike upstream's per-`rcu_state` hierarchy of nodes).
pub static RCU_STATE: RcuState = RcuState::new();

/// Per-CPU quiescent-state snapshot table — analog of `rcu_data.gp_seq`
/// (the `gp_seq` value each CPU has most recently passed a quiescent
/// state for). Kept behind a single lock rather than a true per-CPU atomic
/// array since `MAX_CPUS` (256) makes a const-initialized atomic array
/// awkward and this path is not hot.
static CPU_QS: Mutex<[u64; MAX_CPUS]> = Mutex::new([0; MAX_CPUS]);

/// Initialize tree-RCU state. Must run before any `rcu_read_lock`,
/// `call_rcu`, or `synchronize_rcu` call.
pub fn init() {
    RCU_STATE.gp_seq.store(0, Ordering::Release);
    RCU_STATE.gp_seq_start.store(0, Ordering::Release);
    RCU_STATE.set_state(GpState::Idle);
    RCU_STATE.gp_requested.store(false, Ordering::Release);
    let mut qs = CPU_QS.lock();
    for slot in qs.iter_mut() {
        *slot = 0;
    }
}

/// True while a grace period is in progress (`gp_seq` is odd), the same
/// test Linux performs via `rcu_seq_state(gp_seq)`.
pub fn gp_in_progress() -> bool {
    RCU_STATE.gp_seq.load(Ordering::Acquire) & 1 == 1
}

/// Snapshot the sequence number a new callback/`synchronize_rcu` caller
/// must wait for — analog of `rcu_seq_snap()`. If a grace period is
/// already running, the *next* completion (current + 1) suffices. If RCU
/// is idle, a caller racing with `call_rcu` could still be appending after
/// a grace period has already been sampled as started, so we conservatively
/// require a full subsequent grace period (current + 2).
pub fn rcu_seq_snap() -> u64 {
    let seq = RCU_STATE.gp_seq.load(Ordering::Acquire);
    let target = if seq & 1 == 1 { seq + 1 } else { seq + 2 };
    RCU_STATE.gp_requested.store(true, Ordering::Release);
    target
}

/// Poll whether the grace period identified by `oldstate` (as returned by
/// [`rcu_seq_snap`]) has completed — analog of `poll_state_synchronize_rcu`.
pub fn poll_state(oldstate: u64) -> bool {
    RCU_STATE.gp_seq.load(Ordering::Acquire) >= oldstate
}

/// Record that the current CPU has passed through a quiescent state.
/// Called from `rcu_read_unlock()` when nesting reaches zero and from
/// `rcu_note_context_switch()`.
pub fn note_quiescent_state_for_current_cpu() {
    let cpu = (crate::smp::current_cpu() as usize).min(MAX_CPUS - 1);
    let gp = RCU_STATE.gp_seq.load(Ordering::Acquire);
    CPU_QS.lock()[cpu] = gp;
}

/// True if every online CPU has reported a quiescent state at or after
/// `start` — analog of `rcu_gp_fqs()`'s per-CPU bitmap check, simplified
/// to a linear scan since this kernel's CPU counts are small.
fn all_cpus_quiescent_since(start: u64) -> bool {
    let qs = CPU_QS.lock();
    let online = crate::smp::online_cpus().min(MAX_CPUS as u32);
    for cpu in 0..online as usize {
        if qs[cpu] < start {
            return false;
        }
    }
    true
}

/// Advance the grace-period state machine by one step. Returns `true` if
/// a grace period completed as a result of this call (i.e. callbacks may
/// now be safe to invoke).
///
/// Mirrors the upstream `rcu_gp_kthread()` loop:
///   Idle      -> rcu_gp_init()    (start a new GP, latch gp_seq_start)
///   Init      -> (first) rcu_gp_fqs() sweep
///   FqsSweep  -> repeated rcu_gp_fqs() until all CPUs quiescent
///   Cleanup   -> rcu_gp_cleanup()  (end the GP, gp_seq += 1, back to Idle)
pub fn gp_machine_step() -> bool {
    match RCU_STATE.state() {
        GpState::Idle => {
            if RCU_STATE.gp_requested.swap(false, Ordering::AcqRel) {
                // rcu_gp_init(): start a new grace period.
                let started = RCU_STATE.gp_seq.fetch_add(1, Ordering::AcqRel) + 1;
                RCU_STATE.gp_seq_start.store(started, Ordering::Release);
                RCU_STATE.set_state(GpState::Init);
            }
            false
        }
        GpState::Init => {
            // rcu_gp_init() has set up the GP; begin force-quiescent-state
            // sweeps.
            RCU_STATE.set_state(GpState::FqsSweep);
            false
        }
        GpState::FqsSweep => {
            // rcu_gp_fqs(): poke each CPU and check whether it has
            // reported a quiescent state since this GP started.
            RCU_STATE.n_force_qs.fetch_add(1, Ordering::Relaxed);
            let start = RCU_STATE.gp_seq_start.load(Ordering::Acquire);
            if all_cpus_quiescent_since(start) {
                RCU_STATE.set_state(GpState::Cleanup);
            }
            false
        }
        GpState::Cleanup => {
            // rcu_gp_cleanup(): all CPUs quiesced — end the grace period.
            RCU_STATE.gp_seq.fetch_add(1, Ordering::AcqRel);
            RCU_STATE.gp_count.fetch_add(1, Ordering::Relaxed);
            RCU_STATE.set_state(GpState::Idle);
            true
        }
    }
}

/// Request that a new grace period be started the next time the state
/// machine is idle (used directly by SRCU-adjacent callers that don't go
/// through `call_rcu`/`synchronize_rcu`'s sequence-snapshot path).
pub fn request_gp() {
    RCU_STATE.gp_requested.store(true, Ordering::Release);
}

/// Number of completed grace periods so far.
pub fn gp_count() -> u64 {
    RCU_STATE.gp_count.load(Ordering::Relaxed)
}

/// Number of force-quiescent-state sweeps performed so far.
pub fn n_force_qs() -> u64 {
    RCU_STATE.n_force_qs.load(Ordering::Relaxed)
}

/// Current raw `gp_seq` value (for diagnostics).
pub fn gp_seq() -> u64 {
    RCU_STATE.gp_seq.load(Ordering::Acquire)
}
