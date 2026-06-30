// SPDX-License-Identifier: GPL-2.0-compatible
//! Minimal debug-only lock-class / lock-ordering tracker ("lockdep-lite").
//!
//! A full port of `kernel/locking/lockdep.c` (its dependency graph,
//! per-class statistics, and irq-context tracking) is out of scope. This
//! is a much smaller debug-only checker covering the two violation
//! classes that are cheap to detect and have already bitten this
//! codebase once (see `src/futex.rs`'s `cmp_requeue_pi` comment at the
//! "Acquire bucket locks in index order to prevent ABBA deadlock" site,
//! which documents manually avoiding exactly the bug this module can
//! catch automatically):
//!
//! 1. **Double-lock, same task**: a task tries to acquire a lock it
//!    already holds (self-deadlock on a non-reentrant lock).
//! 2. **AB-BA ordering violations**: task 1 acquires lock A then B while
//!    some other observed acquisition sequence acquired B then A,
//!    indicating two code paths can deadlock against each other.
//!
//! This is gated on the Cargo feature `lockdep` (see `[features]` in
//! `Cargo.toml`), **not** `#[cfg(debug_assertions)]`. This target's
//! `.cargo/config.toml` sets `-C debug-assertions=off` unconditionally
//! (even for debug profile builds), so `cfg(debug_assertions)` is always
//! false here and would make this checker permanently dead code. Enable
//! it explicitly with `cargo build --features lockdep ...`. With the
//! feature off (the default), every function in this module is a
//! zero-cost no-op (parameters are accepted and dropped), so callers can
//! unconditionally call `lockdep::acquire(..)` / `lockdep::release(..)`
//! from lock/unlock paths without `#[cfg]` clutter at the call site.
//!
//! This module is intentionally **not** wired into every lock type in
//! `src/locking/` — only [`crate::locking::mutex::Mutex`] and
//! [`crate::locking::rtmutex::RtMutex`] call into it, as a proof that the
//! hooks are exercised. Wiring it into `rwsem`/`semaphore`/`completion`
//! is mechanical (same two calls) and left as follow-up.

#![allow(dead_code)]

/// Opaque identifier for one lock *instance* (its address). Two different
/// `Mutex<T>` instances are different "classes" here — this is coarser
/// than upstream (which keys by lock *site*, not by instance), but is
/// sufficient to catch double-lock and AB-BA bugs against concrete lock
/// objects, which is what this module targets.
pub type LockId = u64;

/// Compute a [`LockId`] for any lock object from its address.
#[inline]
pub fn lock_id_of<T>(lock: &T) -> LockId {
    lock as *const T as usize as u64
}

#[cfg(feature = "lockdep")]
mod imp {
    use super::LockId;
    use alloc::collections::{BTreeMap, BTreeSet};
    use alloc::vec::Vec;
    use spin::Mutex as SpinMutex;

    /// Per-task stack of currently-held (lock_id, name) pairs.
    static HELD: SpinMutex<BTreeMap<usize, Vec<(LockId, &'static str)>>> =
        SpinMutex::new(BTreeMap::new());

    /// Observed "A acquired-before B" ordering edges across all tasks.
    static ORDER: SpinMutex<BTreeSet<(LockId, LockId)>> = SpinMutex::new(BTreeSet::new());

    pub fn acquire(task_id: usize, lock_id: LockId, name: &'static str) {
        let mut held = HELD.lock();
        let stack = held.entry(task_id).or_insert_with(Vec::new);

        // 1) Double-lock detection: this task already holds this exact lock.
        if stack.iter().any(|&(id, _)| id == lock_id) {
            panic!(
                "lockdep: task {} double-acquired lock '{}' ({:#x})",
                task_id, name, lock_id
            );
        }

        // 2) AB-BA ordering check + edge recording.
        {
            let mut order = ORDER.lock();
            for &(held_id, held_name) in stack.iter() {
                if held_id == lock_id {
                    continue;
                }
                // If we've ever seen lock_id acquired-before held_id, then
                // acquiring held_id-before-lock_id now (current order) is
                // the reverse of a previously observed order: ABBA.
                if order.contains(&(lock_id, held_id)) {
                    panic!(
                        "lockdep: ABBA lock-order violation: '{}' ({:#x}) -> '{}' ({:#x}) \
                         conflicts with a previously observed reverse order",
                        held_name, held_id, name, lock_id
                    );
                }
                order.insert((held_id, lock_id));
            }
        }

        stack.push((lock_id, name));
    }

    pub fn release(task_id: usize, lock_id: LockId) {
        let mut held = HELD.lock();
        if let Some(stack) = held.get_mut(&task_id) {
            if let Some(pos) = stack.iter().position(|&(id, _)| id == lock_id) {
                stack.remove(pos);
            }
        }
    }
}

/// Record that `task_id` is acquiring lock `lock_id` (named `name` for
/// diagnostics). Panics on a detected double-lock or AB-BA violation.
///
/// No-op in release builds.
#[inline]
pub fn acquire(task_id: usize, lock_id: LockId, name: &'static str) {
    #[cfg(feature = "lockdep")]
    imp::acquire(task_id, lock_id, name);
    #[cfg(not(feature = "lockdep"))]
    {
        let _ = (task_id, lock_id, name);
    }
}

/// Record that `task_id` released lock `lock_id`. No-op in release builds.
#[inline]
pub fn release(task_id: usize, lock_id: LockId) {
    #[cfg(feature = "lockdep")]
    imp::release(task_id, lock_id);
    #[cfg(not(feature = "lockdep"))]
    {
        let _ = (task_id, lock_id);
    }
}
