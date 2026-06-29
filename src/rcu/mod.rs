//! Read-copy-update (RCU) — simplified grace-period tracking and deferred free.
//!
//! Callbacks queued via [`call_rcu`] run from the RCU softirq once a grace
//! period completes (all online CPUs have quiesced).

extern crate alloc;

use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::Mutex;

use crate::softirq::{raise_softirq, register_softirq, RCU_SOFTIRQ};

/// Deferred callback invoked after a grace period.
pub type RcuCallback = fn(*mut u8);

struct RcuHead {
    func: RcuCallback,
    arg: usize,
}

unsafe impl Send for RcuHead {}
unsafe impl Sync for RcuHead {}

/// Per-CPU reader nesting depth (simplified: global counter on single-CPU boot path).
static READ_NESTING: AtomicUsize = AtomicUsize::new(0);

/// Monotonic grace-period sequence — incremented on each `synchronize_rcu`.
static GP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Last grace period each CPU has observed (quiescent snapshot).
static CPU_QS: Mutex<[u64; crate::smp::MAX_CPUS]> = Mutex::new([0; crate::smp::MAX_CPUS]);

static CALLBACKS: Mutex<VecDeque<RcuHead>> = Mutex::new(VecDeque::new());
static CALLBACKS_DONE: AtomicU64 = AtomicU64::new(0);

fn rcu_softirq_action() {
    process_callbacks();
}

/// Initialize RCU and hook the RCU softirq vector.
pub fn init() {
    register_softirq(RCU_SOFTIRQ, rcu_softirq_action);
    GP_SEQ.store(1, Ordering::Release);
    for cpu in 0..crate::smp::MAX_CPUS {
        CPU_QS.lock()[cpu] = GP_SEQ.load(Ordering::Acquire);
    }
    crate::serial_println!("[rcu] initialized (grace-period RCU)");
}

/// Enter an RCU read-side critical section.
#[inline]
pub fn rcu_read_lock() {
    READ_NESTING.fetch_add(1, Ordering::AcqRel);
}

/// Leave an RCU read-side critical section.
#[inline]
pub fn rcu_read_unlock() {
    READ_NESTING.fetch_sub(1, Ordering::AcqRel);
}

/// True when no CPU holds an RCU read lock (single-CPU approximation).
pub fn rcu_quiescent_state() -> bool {
    READ_NESTING.load(Ordering::Acquire) == 0
}

/// Mark the current CPU as having passed through a quiescent state.
pub fn rcu_note_context_switch() {
    let cpu = crate::smp::current_cpu() as usize;
    if cpu < crate::smp::MAX_CPUS {
        CPU_QS.lock()[cpu] = GP_SEQ.load(Ordering::Acquire);
    }
}

fn all_cpus_quiescent(gp: u64) -> bool {
    let qs = CPU_QS.lock();
    let online = crate::smp::online_cpus().min(crate::smp::MAX_CPUS as u32);
    for cpu in 0..online as usize {
        if qs[cpu] < gp {
            return false;
        }
    }
    true
}

fn process_callbacks() {
    let gp = GP_SEQ.load(Ordering::Acquire);
    if !all_cpus_quiescent(gp) {
        raise_softirq(RCU_SOFTIRQ);
        return;
    }

    loop {
        let head = CALLBACKS.lock().pop_front();
        let Some(head) = head else {
            break;
        };
        (head.func)(head.arg as *mut u8);
        CALLBACKS_DONE.fetch_add(1, Ordering::Relaxed);
    }
}

/// Queue `func(arg)` to run after the next grace period.
pub fn call_rcu(func: RcuCallback, arg: *mut u8) {
    CALLBACKS.lock().push_back(RcuHead {
        func,
        arg: arg as usize,
    });
    GP_SEQ.fetch_add(1, Ordering::AcqRel);
    raise_softirq(RCU_SOFTIRQ);
}

/// Block until all pre-existing RCU callbacks have completed.
pub fn synchronize_rcu() {
    let target = GP_SEQ.fetch_add(1, Ordering::AcqRel) + 1;
    rcu_note_context_switch();

    for _ in 0..1_000_000 {
        if all_cpus_quiescent(target) {
            process_callbacks();
            return;
        }
        core::hint::spin_loop();
    }
    process_callbacks();
}

/// Number of callbacks executed so far (statistics).
pub fn callbacks_completed() -> u64 {
    CALLBACKS_DONE.load(Ordering::Relaxed)
}

/// Pending callback count.
pub fn callbacks_pending() -> usize {
    CALLBACKS.lock().len()
}
