//! RCU read-side API, callback registration, and the cooperative
//! "grace-period kthread" stand-in.
//!
//! Ported (simplified) from Linux `kernel/rcu/update.c` (the read-side API
//! and `synchronize_rcu`/`call_rcu` entry points) and the non-preemptible
//! callback-draining half of `kernel/rcu/tree.c`. RustOS has no RCU
//! kthread, so grace-period progress is instead driven by the RCU softirq
//! (`process_callbacks`, registered against [`crate::softirq::RCU_SOFTIRQ`])
//! and by `synchronize_rcu()` spinning on [`tree::gp_machine_step`].

use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

use crate::smp::MAX_CPUS;
use crate::softirq::{raise_softirq, register_softirq, RCU_SOFTIRQ};

use super::tree;

/// Deferred callback invoked after a grace period — matches the original
/// `RcuCallback` signature so existing callers don't need to change.
pub type RcuCallback = fn(*mut u8);

/// A queued callback, analog of `struct rcu_head` plus the target grace
/// period it is waiting on (Linux instead keeps callbacks bucketed by
/// `gp_seq` in `rcu_segcblist`; we keep a flat queue and tag each entry).
struct RcuHead {
    func: RcuCallback,
    arg: usize,
    /// `gp_seq` value (as returned by [`tree::rcu_seq_snap`]) that must be
    /// reached before this callback may run.
    target_seq: u64,
}

unsafe impl Send for RcuHead {}
unsafe impl Sync for RcuHead {}

const ZERO_NESTING: AtomicUsize = AtomicUsize::new(0);

/// Per-CPU RCU read-side nesting depth — analog of the nesting counter
/// Linux keeps in `task_struct.rcu_read_lock_nesting` (tiny/non-preemptible
/// builds instead keep it per-CPU, which is what this flat, non-preemptible
/// kernel approximates). Indexed by `smp::current_cpu()`.
static READ_NESTING: [AtomicUsize; MAX_CPUS] = [ZERO_NESTING; MAX_CPUS];

static CALLBACKS: Mutex<VecDeque<RcuHead>> = Mutex::new(VecDeque::new());
static CALLBACKS_DONE: AtomicU64 = AtomicU64::new(0);

#[inline]
fn this_cpu() -> usize {
    (crate::smp::current_cpu() as usize).min(MAX_CPUS - 1)
}

fn rcu_softirq_action() {
    process_callbacks();
}

/// Initialize RCU: bring up the grace-period state machine and hook the
/// RCU softirq vector.
pub fn init() {
    tree::init();
    register_softirq(RCU_SOFTIRQ, rcu_softirq_action);
    crate::serial_println!("[rcu] initialized (tree-based grace-period RCU)");
}

/// Enter an RCU read-side critical section. Nests correctly per CPU.
#[inline]
pub fn rcu_read_lock() {
    READ_NESTING[this_cpu()].fetch_add(1, Ordering::AcqRel);
}

/// Leave an RCU read-side critical section. When nesting drops to zero the
/// current CPU is recorded as quiescent for the in-progress grace period,
/// mirroring `__rcu_read_unlock()`'s `rcu_read_unlock_special()` fast path.
#[inline]
pub fn rcu_read_unlock() {
    let cpu = this_cpu();
    let prev = READ_NESTING[cpu].fetch_sub(1, Ordering::AcqRel);
    debug_assert!(
        prev > 0,
        "rcu_read_unlock() called without a matching rcu_read_lock()"
    );
    if prev == 1 {
        tree::note_quiescent_state_for_current_cpu();
    }
}

/// Current CPU's RCU read-side nesting depth (0 = not in a read-side
/// critical section). Analog of `rcu_preempt_depth()`.
pub fn rcu_read_lock_nesting() -> usize {
    READ_NESTING[this_cpu()].load(Ordering::Acquire)
}

/// True when the *current* CPU holds no RCU read lock. Note this is a
/// per-CPU predicate (Linux's `rcu_is_watching`-adjacent checks are also
/// per-CPU); it does not imply other CPUs are quiescent.
pub fn rcu_quiescent_state() -> bool {
    rcu_read_lock_nesting() == 0
}

/// Explicitly mark the current CPU as having passed through a quiescent
/// state, e.g. from a scheduler tick or idle-loop hook — analog of
/// `rcu_note_context_switch()` / `rcu_qs()`.
pub fn rcu_note_context_switch() {
    tree::note_quiescent_state_for_current_cpu();
}

/// Snapshot the current grace-period sequence for a later
/// [`poll_state_synchronize_rcu`] check — analog of
/// `get_state_synchronize_rcu()`.
pub fn get_state_synchronize_rcu() -> u64 {
    tree::rcu_seq_snap()
}

/// Poll whether the grace period identified by `oldstate` has completed —
/// analog of `poll_state_synchronize_rcu()`. Non-blocking.
pub fn poll_state_synchronize_rcu(oldstate: u64) -> bool {
    tree::poll_state(oldstate)
}

/// Drain the per-CPU(-ish) callback queue: advance the grace-period state
/// machine one step, then run every callback whose target grace period has
/// completed. Re-arms the softirq if work remains, analog of
/// `rcu_do_batch()` being re-invoked from `rcu_core()`.
fn process_callbacks() {
    tree::gp_machine_step();
    let completed = tree::gp_seq();

    let mut ready: VecDeque<RcuHead> = VecDeque::new();
    {
        let mut cbs = CALLBACKS.lock();
        let mut remaining = VecDeque::with_capacity(cbs.len());
        while let Some(head) = cbs.pop_front() {
            if head.target_seq <= completed {
                ready.push_back(head);
            } else {
                remaining.push_back(head);
            }
        }
        *cbs = remaining;
    }

    let more_pending = !CALLBACKS.lock().is_empty();

    for head in ready {
        (head.func)(head.arg as *mut u8);
        CALLBACKS_DONE.fetch_add(1, Ordering::Relaxed);
    }

    if more_pending || tree::gp_in_progress() {
        raise_softirq(RCU_SOFTIRQ);
    }
}

/// Queue `func(arg)` to run after the next grace period completes —
/// analog of `call_rcu()`. Non-blocking.
pub fn call_rcu(func: RcuCallback, arg: *mut u8) {
    let target = tree::rcu_seq_snap();
    CALLBACKS.lock().push_back(RcuHead {
        func,
        arg: arg as usize,
        target_seq: target,
    });
    raise_softirq(RCU_SOFTIRQ);
}

/// Block until every RCU read-side critical section that was already in
/// progress when this was called has completed — analog of
/// `synchronize_rcu()`. Poll-based: repeatedly drives the grace-period
/// state machine (in lieu of a dedicated `rcu_sched` kthread) until the
/// snapshotted target sequence is reached.
pub fn synchronize_rcu() {
    let target = tree::rcu_seq_snap();

    // The calling context cannot itself be inside an RCU read-side
    // critical section (same contract as upstream), so it is inherently
    // quiescent right now — record that immediately, same as upstream's
    // `synchronize_rcu()` -> `wait_rcu_gp()` relying on the caller having
    // already passed through `rcu_read_unlock()`.
    tree::note_quiescent_state_for_current_cpu();
    raise_softirq(RCU_SOFTIRQ);

    const MAX_SPINS: u64 = 10_000_000;
    let mut spins = 0u64;
    while !tree::poll_state(target) {
        tree::gp_machine_step();
        core::hint::spin_loop();
        spins += 1;
        if spins > MAX_SPINS {
            // Safety valve: avoid hanging forever if called before RCU is
            // fully initialized or if no other CPU is making progress.
            break;
        }
    }
    process_callbacks();
}

/// Number of callbacks executed so far (statistics).
pub fn callbacks_completed() -> u64 {
    CALLBACKS_DONE.load(Ordering::Relaxed)
}

/// Pending callback count (statistics).
pub fn callbacks_pending() -> usize {
    CALLBACKS.lock().len()
}
