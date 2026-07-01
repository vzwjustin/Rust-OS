//! Read-copy-update (RCU) — grace-period tracking, deferred free, and SRCU.
//!
//! Ported (simplified) from Linux `kernel/rcu/{tree.c,update.c,srcu.c}`,
//! split to mirror that layout:
//!   - [`tree`]   — the grace-period state machine (`gp_seq`, force-
//!                  quiescent-state sweep, per-CPU quiescent snapshots).
//!   - [`update`] — the public read-side API, `call_rcu`/`synchronize_rcu`,
//!                  and the softirq-driven callback drainer.
//!   - [`srcu`]   — sleepable RCU (`SrcuStruct`), for subsystems (cgroup,
//!                  the device model) whose read-side sections may block.
//!
//! This kernel has no dedicated `rcu_sched`/`rcu_preempt` kthread, so the
//! grace-period state machine is driven cooperatively: each RCU softirq
//! firing (and each spin of `synchronize_rcu()`) advances it by one step.
//! Callbacks queued via [`call_rcu`] run from the RCU softirq once the
//! grace period they were registered against completes.

extern crate alloc;

pub mod srcu;
pub mod tree;
pub mod update;

pub use srcu::{srcu_read_lock, srcu_read_unlock, synchronize_srcu, SrcuIdx, SrcuStruct};
pub use update::{
    call_rcu, callbacks_completed, callbacks_pending, get_state_synchronize_rcu, init,
    poll_state_synchronize_rcu, rcu_note_context_switch, rcu_quiescent_state, rcu_read_lock,
    rcu_read_lock_nesting, rcu_read_unlock, synchronize_rcu, RcuCallback,
};
