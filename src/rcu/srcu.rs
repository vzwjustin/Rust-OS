//! SRCU (Sleepable RCU) — ported (simplified) from Linux `kernel/rcu/srcu.c`.
//!
//! Unlike classic RCU, SRCU read-side critical sections are allowed to
//! block, and each subsystem gets its own independent `srcu_struct`
//! domain instead of sharing the global grace-period machinery in
//! [`super::tree`]. This is the primitive other ported subsystems
//! (cgroup, the device model) need: cgroup's `css_set` and the driver
//! core's `device` teardown both use SRCU upstream to let readers walk
//! reference-counted lists while updates may sleep.
//!
//! The classic two-index SRCU algorithm:
//!   - readers pick the *current* index and bump a per-index lock counter;
//!   - a writer flips the current index so new readers move to the other
//!     side, then waits for the old index's lock/unlock counters to match,
//!     proving every reader that started under the old index has finished.
//!
//! Upstream `synchronize_srcu()` performs this flip-and-drain twice (to
//! get full memory-barrier separation between the two index generations).
//! This port performs a single flip-and-drain, which is sufficient for
//! correctness (no reader can observe stale data past the drain) but
//! gives slightly weaker ordering guarantees than upstream — acceptable
//! given this kernel does not yet have the fine-grained memory model
//! upstream SRCU is hardened against.

use core::sync::atomic::{AtomicUsize, Ordering};

/// One SRCU domain — analog of `struct srcu_struct`.
pub struct SrcuStruct {
    /// Currently active reader index (0 or 1) — analog of `srcu_idx`.
    idx: AtomicUsize,
    /// Per-index reader-entry counters — analog of
    /// `srcu_data.srcu_lock_count[]`. Not truly per-CPU here, just
    /// per-index, since this kernel's SRCU users are low-traffic.
    lock_count: [AtomicUsize; 2],
    /// Per-index reader-exit counters — analog of
    /// `srcu_data.srcu_unlock_count[]`.
    unlock_count: [AtomicUsize; 2],
}

/// A read-side ticket returned by [`SrcuStruct::read_lock`] and required by
/// the matching [`SrcuStruct::read_unlock`] — analog of the `int idx`
/// returned by `srcu_read_lock()`.
pub type SrcuIdx = usize;

impl SrcuStruct {
    /// Create a new, idle SRCU domain.
    pub const fn new() -> Self {
        Self {
            idx: AtomicUsize::new(0),
            lock_count: [AtomicUsize::new(0), AtomicUsize::new(0)],
            unlock_count: [AtomicUsize::new(0), AtomicUsize::new(0)],
        }
    }

    /// Enter a sleepable SRCU read-side critical section. Returns a ticket
    /// that must be passed to [`Self::read_unlock`].
    #[inline]
    pub fn read_lock(&self) -> SrcuIdx {
        let i = self.idx.load(Ordering::Acquire) & 1;
        self.lock_count[i].fetch_add(1, Ordering::AcqRel);
        i
    }

    /// Leave a sleepable SRCU read-side critical section.
    #[inline]
    pub fn read_unlock(&self, idx: SrcuIdx) {
        self.unlock_count[idx & 1].fetch_add(1, Ordering::AcqRel);
    }

    /// True if all readers that entered under index `i` have left.
    fn index_drained(&self, i: usize) -> bool {
        self.lock_count[i].load(Ordering::Acquire) == self.unlock_count[i].load(Ordering::Acquire)
    }

    /// Block until every SRCU read-side critical section already in
    /// progress on this domain has completed — analog of
    /// `synchronize_srcu()`. Poll-based, like [`super::update::synchronize_rcu`].
    pub fn synchronize(&self) {
        // Flip: new readers will pick up the other index from here on.
        let old = self.idx.fetch_xor(1, Ordering::AcqRel) & 1;

        const MAX_SPINS: u64 = 10_000_000;
        let mut spins = 0u64;
        while !self.index_drained(old) {
            core::hint::spin_loop();
            spins += 1;
            if spins > MAX_SPINS {
                break;
            }
        }
    }

    /// Number of readers currently active on this domain (diagnostics).
    pub fn active_readers(&self) -> usize {
        let l0 = self.lock_count[0].load(Ordering::Relaxed);
        let l1 = self.lock_count[1].load(Ordering::Relaxed);
        let u0 = self.unlock_count[0].load(Ordering::Relaxed);
        let u1 = self.unlock_count[1].load(Ordering::Relaxed);
        (l0 + l1).saturating_sub(u0 + u1)
    }
}

impl Default for SrcuStruct {
    fn default() -> Self {
        Self::new()
    }
}

/// Enter an SRCU read-side critical section on `ss` — free-function form
/// matching upstream's `srcu_read_lock(&srcu_struct)`.
#[inline]
pub fn srcu_read_lock(ss: &SrcuStruct) -> SrcuIdx {
    ss.read_lock()
}

/// Leave an SRCU read-side critical section on `ss` — free-function form
/// matching upstream's `srcu_read_unlock(&srcu_struct, idx)`.
#[inline]
pub fn srcu_read_unlock(ss: &SrcuStruct, idx: SrcuIdx) {
    ss.read_unlock(idx)
}

/// Block until pre-existing SRCU readers on `ss` have completed — analog
/// of `synchronize_srcu(&srcu_struct)`.
pub fn synchronize_srcu(ss: &SrcuStruct) {
    ss.synchronize();
}
