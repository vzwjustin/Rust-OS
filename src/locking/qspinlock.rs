// SPDX-License-Identifier: GPL-2.0-compatible
//! Queued (MCS-style) spinlock.
//!
//! Mirrors `kernel/locking/qspinlock.c`: a more scalable replacement for a
//! naive test-and-set spinlock on SMP.  Lock state is split into the same
//! four logical states Linux's `qspinlock` uses:
//!
//! 1. **Uncontended**: `locked == 0`, no pending waiter, empty queue tail.
//!    A CAS directly grants the lock.
//! 2. **Pending**: exactly one extra waiter is spinning directly on the
//!    `locked` byte ("pending" bit set) without joining the MCS queue —
//!    Linux's single-waiter fast path that avoids queueing overhead for
//!    the common two-CPU contention case.
//! 3. **Queued (locked)**: two or more waiters; the second and subsequent
//!    waiters enqueue an MCS node and spin on their own cache line
//!    (`node.locked`) until their predecessor hands off, which keeps
//!    contention traffic off the shared lock word.
//! 4. **Locked**: the lock owner holds `locked == 1`; on unlock the byte
//!    is simply cleared (handoff to the next waiter happens when *that*
//!    waiter wins the race for the byte, not via direct transfer).
//!
//! ## Deliberate simplifications vs. upstream
//!
//! - Upstream packs `locked | pending | tail` into a single `atomic_t`
//!   (4 bytes) for a single atomic CAS/xchg per transition. Here the
//!   three pieces of state are tracked in separate atomics
//!   (`AtomicU8` + `AtomicBool` + `AtomicU32`). This trades upstream's
//!   single-instruction state transitions for much simpler, easier to
//!   verify Rust code; the queued-spinlock *protocol* (four states,
//!   MCS hand-off, pending fast path) is preserved faithfully.
//! - Upstream allocates 4 per-CPU MCS nodes (one per interrupt nesting
//!   level: process/softirq/hardirq/nmi) so a CPU can hold multiple
//!   outstanding qspinlock queue positions across interrupt contexts.
//!   This port has exactly one MCS node per CPU
//!   (`NODES[current_cpu()]`), so a qspinlock must not be acquired
//!   re-entrantly from an interrupt handler while the same CPU is
//!   already queued for it in process context. RustOS does not yet
//!   have a generic IRQ-context lock-nesting tracker, so this is
//!   documented as a known limitation rather than worked around.
//!
//! Since this is a spinlock, `lock()` busy-waits by design; it must
//! never call into the scheduler.

#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use crate::smp::MAX_CPUS;

// ---------------------------------------------------------------------------
// Per-CPU MCS queue nodes
// ---------------------------------------------------------------------------

struct McsNode {
    /// Set by our predecessor once it is our turn to compete for the lock.
    locked: AtomicBool,
    /// `0` = no successor yet; otherwise `cpu_id + 1` of the successor.
    next: AtomicU32,
}

impl McsNode {
    const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            next: AtomicU32::new(0),
        }
    }
}

/// One MCS queue slot per CPU (see "Deliberate simplifications" above for
/// why this is one slot rather than four nesting-level slots).
static NODES: [McsNode; MAX_CPUS] = {
    const N: McsNode = McsNode::new();
    [N; MAX_CPUS]
};

// ---------------------------------------------------------------------------
// QSpinLock
// ---------------------------------------------------------------------------

/// A queued (MCS-style) spinlock.
///
/// Use this in place of a naive `AtomicBool`/`spin::Mutex`-style lock on
/// paths with real SMP contention, where queueing (rather than all CPUs
/// hammering one cache line) materially improves scalability.
pub struct QSpinLock {
    /// `0` = unlocked, `1` = locked.
    locked: AtomicU8,
    /// Single-waiter fast-path bit (state 2 above).
    pending: AtomicBool,
    /// MCS queue tail: `0` = empty, else `cpu_id + 1` of the tail CPU.
    tail: AtomicU32,
}

unsafe impl Send for QSpinLock {}
unsafe impl Sync for QSpinLock {}

impl QSpinLock {
    /// Create a new, unlocked queued spinlock.
    pub const fn new() -> Self {
        Self {
            locked: AtomicU8::new(0),
            pending: AtomicBool::new(false),
            tail: AtomicU32::new(0),
        }
    }

    /// Returns true if currently locked (best-effort, racy by nature).
    #[inline]
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed) != 0
    }

    /// Try to acquire the lock without spinning or queueing.
    ///
    /// Fails (rather than racing) whenever *any* contention is already
    /// known — a pending waiter or a non-empty queue — exactly mirroring
    /// `queued_spin_trylock()`'s "if the word is non-zero, bail" rule, so
    /// a successful direct-CAS racer can never sneak the lock away from a
    /// waiter that already owns the pending slot.
    pub fn try_lock(&self) -> bool {
        if self.pending.load(Ordering::Relaxed) || self.tail.load(Ordering::Relaxed) != 0 {
            return false;
        }
        self.locked
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// Acquire the lock, spinning until available.
    pub fn lock(&self) {
        if self.try_lock() {
            return;
        }
        self.lock_contended();
    }

    /// Release the lock.
    pub fn unlock(&self) {
        self.locked.store(0, Ordering::Release);
    }

    // -----------------------------------------------------------------------
    // Slow path
    // -----------------------------------------------------------------------

    #[cold]
    fn lock_contended(&self) {
        // State 2: pending fast path. Only valid while the MCS queue is
        // empty; if a queue already exists we must join it to preserve
        // (rough) FIFO-ish fairness, matching upstream's `node.tail`
        // check before taking the pending slot.
        if self.tail.load(Ordering::Relaxed) == 0 && !self.pending.swap(true, Ordering::Acquire) {
            // We now own the single pending slot. Spin directly on the
            // lock byte (no queueing overhead) until it is free.
            while self.locked.load(Ordering::Acquire) != 0 {
                core::hint::spin_loop();
            }
            self.locked.store(1, Ordering::Relaxed);
            self.pending.store(false, Ordering::Release);
            return;
        }

        // State 3: queued path. Join the MCS chain.
        let cpu = crate::smp::current_cpu();
        let node = &NODES[cpu as usize];
        node.next.store(0, Ordering::Relaxed);
        node.locked.store(false, Ordering::Relaxed);

        let prev_tail = self.tail.swap(cpu + 1, Ordering::AcqRel);
        if prev_tail != 0 {
            // Link behind our predecessor and wait for it to hand off our
            // turn to compete for the lock.
            let prev = &NODES[(prev_tail - 1) as usize];
            prev.next.store(cpu + 1, Ordering::Release);
            while !node.locked.load(Ordering::Acquire) {
                core::hint::spin_loop();
            }
        }

        // We are now the head of the MCS queue: compete for the actual
        // lock byte (state 4), skipping the pending bit since we already
        // queued.
        loop {
            if self.locked.load(Ordering::Relaxed) == 0 && !self.pending.load(Ordering::Relaxed) {
                if self
                    .locked
                    .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
                {
                    break;
                }
            }
            core::hint::spin_loop();
        }

        // Hand off our MCS queue position to the next waiter (if any),
        // letting it start competing for the lock byte too.
        let mut next = node.next.load(Ordering::Acquire);
        if next == 0 {
            if self
                .tail
                .compare_exchange(cpu + 1, 0, Ordering::AcqRel, Ordering::Relaxed)
                .is_err()
            {
                // A successor is concurrently linking itself; wait for it.
                loop {
                    next = node.next.load(Ordering::Acquire);
                    if next != 0 {
                        break;
                    }
                    core::hint::spin_loop();
                }
            }
        }
        if next != 0 {
            NODES[(next - 1) as usize]
                .locked
                .store(true, Ordering::Release);
        }
    }
}

impl Default for QSpinLock {
    fn default() -> Self {
        Self::new()
    }
}
