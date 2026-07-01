// SPDX-License-Identifier: GPL-2.0-compatible
//! Percpu reader-writer semaphore.
//!
//! Mirrors `kernel/locking/percpu-rwsem.c` / `include/linux/percpu-rwsem.h`,
//! the primitive Linux uses for very-hot, very-rarely-written locks such as
//! `cgroup_threadgroup_rwsem`: readers are common and should be nearly
//! free, writers are rare and may pay a comparatively expensive cost.
//!
//! ## Honest comparison to upstream
//!
//! Upstream's speed comes from two things this port does **not** fully
//! have:
//!
//! 1. A true per-CPU counter array (one cache line per CPU, no shared
//!    state touched on the read fast path) plus `rcu_sync` so writers
//!    wait out a grace period instead of spinning on a live count.
//! 2. `this_cpu_inc()` — a single non-atomic, preemption-disabled
//!    increment with no cross-CPU traffic at all.
//!
//! This codebase's [`crate::smp::PerCpu`] is `Mutex<BTreeMap<cpu, T>>`-
//! backed (see `src/smp.rs`), so every "per-CPU" increment/decrement here
//! still takes one *global* spinlock, and summing all CPUs' counts (via
//! the `PerCpu::for_each` helper added alongside this file) also locks
//! that same map. We build on `PerCpu` anyway per the task's "only add a
//! percpu-backed primitive if one already exists" rule, because it gives
//! the right *structure* (independent per-CPU reader slots, writer
//! drain) and the right *API*, but it does **not** deliver upstream's
//! contention-free read fast path. Treat this as a correctness/structure
//! port, not a perf-equivalent one. A real win requires a true per-CPU
//! array (e.g. `static [Count; MAX_CPUS]`, mirroring `src/smp.rs`'s own
//! `CPU_DATA`), which is future work.
//!
//! Likewise, `src/rcu/` is owned by another agent on this branch and is
//! off-limits here, so the writer drain below spins on the *summed*
//! per-CPU reader count (after raising `readers_block`, with a
//! double-checked reader fast path that closes the classic
//! percpu-rwsem "writer started concurrently" race) instead of using
//! `rcu_sync`. This mirrors the pre-`rcu_sync` Linux `percpu_rw_semaphore`
//! design (which also spun on a plain atomic counter), so it is a
//! faithful simplification rather than an invented one.

#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, Ordering};

use crate::smp::PerCpu;

/// Percpu reader-writer semaphore.
///
/// Readers should be the hot, common case (e.g. "is the threadgroup
/// stable" checks taken on every `clone()`/`exit()`); writers
/// (threadgroup-changing operations) are rare and pay the cost of
/// draining all outstanding readers.
pub struct PercpuRwSemaphore {
    /// Per-CPU reader counts. Positive while that CPU has active readers.
    counts: PerCpu<i32>,
    /// Set by a writer to force new readers onto the slow (blocking) path.
    /// Mirrors `percpu_rw_semaphore::block`.
    readers_block: AtomicBool,
    /// Serialises writers against each other (`percpu_rw_semaphore` relies
    /// on its embedded `rw_semaphore` for this; a simple exclusion flag
    /// suffices here since only the writer side needs mutual exclusion).
    writer_busy: AtomicBool,
}

// SAFETY: PercpuRwSemaphore uses only atomics and PerCpu (which is itself
// Send+Sync). All interior mutability goes through atomic operations.
unsafe impl Send for PercpuRwSemaphore {}
unsafe impl Sync for PercpuRwSemaphore {}

impl PercpuRwSemaphore {
    /// Create a new, unlocked percpu rwsem.
    pub const fn new() -> Self {
        Self {
            counts: PerCpu::new(),
            readers_block: AtomicBool::new(false),
            writer_busy: AtomicBool::new(false),
        }
    }

    // -----------------------------------------------------------------------
    // Per-CPU counter helpers
    // -----------------------------------------------------------------------

    fn bump(&self, delta: i32) {
        let applied = self.counts.get_mut(|v| *v += delta).is_some();
        if !applied {
            self.counts.set(delta);
        }
    }

    /// Sum every CPU's reader count.
    fn sum(&self) -> i64 {
        let mut total: i64 = 0;
        self.counts.for_each(|_cpu, v| total += *v as i64);
        total
    }

    // -----------------------------------------------------------------------
    // Reader side
    // -----------------------------------------------------------------------

    /// Acquire a shared (reader) lock.
    ///
    /// Mirrors `percpu_down_read()`: the fast path just bumps the
    /// per-CPU counter and rechecks `readers_block` (closing the
    /// writer-started-concurrently race); the slow path backs out and
    /// spins until the writer finishes, then retries.
    pub fn read_lock(&self) {
        loop {
            self.bump(1);
            if !self.readers_block.load(Ordering::Acquire) {
                return;
            }
            // A writer is active (or starting): back out and wait.
            self.bump(-1);
            while self.readers_block.load(Ordering::Acquire) {
                core::hint::spin_loop();
            }
        }
    }

    /// Release a shared (reader) lock. Mirrors `percpu_up_read()`.
    pub fn read_unlock(&self) {
        self.bump(-1);
    }

    // -----------------------------------------------------------------------
    // Writer side
    // -----------------------------------------------------------------------

    /// Acquire an exclusive (writer) lock, draining all current readers.
    ///
    /// Mirrors `percpu_down_write()`. Spin-waits for the summed reader
    /// count to reach zero rather than waiting out an `rcu_sync` grace
    /// period (see module docs for why).
    pub fn write_lock(&self) {
        while self
            .writer_busy
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        self.readers_block.store(true, Ordering::Release);
        loop {
            if self.sum() <= 0 {
                break;
            }
            core::hint::spin_loop();
        }
    }

    /// Release an exclusive (writer) lock. Mirrors `percpu_up_write()`.
    pub fn write_unlock(&self) {
        self.readers_block.store(false, Ordering::Release);
        self.writer_busy.store(false, Ordering::Release);
    }
}

impl Default for PercpuRwSemaphore {
    fn default() -> Self {
        Self::new()
    }
}
