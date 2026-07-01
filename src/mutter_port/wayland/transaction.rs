//! GNOME src/wayland/meta-wayland-transaction.c
//!
//! MetaWaylandTransaction batches surface state changes so a subsurface tree
//! applies atomically. A transaction holds one entry per referenced surface;
//! it is applied once all dependencies (buffers ready, ancestor transactions)
//! are satisfied, ordered by committed sequence.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-transaction.c

use alloc::{collections::BTreeMap, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};

/// Per-surface entry within a transaction (MetaWaylandTransactionEntry).
#[derive(Debug, Clone)]
pub struct MetaWaylandTransactionEntry {
    pub surface_id: u32,
    /// Buffered surface state id captured at commit (None if state-only).
    pub state_id: Option<u32>,
    /// Sub-surface position, if this entry carries one.
    pub has_sub_pos: bool,
    pub x: i32,
    pub y: i32,
    /// Ids of buffers that must become ready before the entry can apply.
    pub buf_dependencies: Vec<u32>,
}

impl MetaWaylandTransactionEntry {
    pub fn new(surface_id: u32) -> Self {
        MetaWaylandTransactionEntry {
            surface_id,
            state_id: None,
            has_sub_pos: false,
            x: 0,
            y: 0,
            buf_dependencies: Vec::new(),
        }
    }

    /// meta_wayland_transaction_entry_merge_into: fold `self` into a newer entry.
    pub fn merge_into(&self, into: &mut MetaWaylandTransactionEntry) {
        if self.has_sub_pos && !into.has_sub_pos {
            into.has_sub_pos = true;
            into.x = self.x;
            into.y = self.y;
        }
        if into.state_id.is_none() {
            into.state_id = self.state_id;
        }
        for dep in &self.buf_dependencies {
            if !into.buf_dependencies.contains(dep) {
                into.buf_dependencies.push(*dep);
            }
        }
    }
}

/// A batch of atomic surface state changes (MetaWaylandTransaction).
pub struct MetaWaylandTransaction {
    pub id: u32,
    pub committed_sequence: u64,
    pub target_presentation_time_us: i64,
    /// surface id -> entry.
    entries: BTreeMap<u32, MetaWaylandTransactionEntry>,
}

impl MetaWaylandTransaction {
    pub fn new(id: u32) -> Self {
        MetaWaylandTransaction {
            id,
            committed_sequence: 0,
            target_presentation_time_us: 0,
            entries: BTreeMap::new(),
        }
    }

    /// meta_wayland_transaction_ensure_entry.
    pub fn ensure_entry(&mut self, surface_id: u32) -> &mut MetaWaylandTransactionEntry {
        self.entries
            .entry(surface_id)
            .or_insert_with(|| MetaWaylandTransactionEntry::new(surface_id))
    }

    pub fn get_entry(&self, surface_id: u32) -> Option<&MetaWaylandTransactionEntry> {
        self.entries.get(&surface_id)
    }

    /// meta_wayland_transaction_add_subsurface_position.
    pub fn add_subsurface_position(&mut self, surface_id: u32, x: i32, y: i32) {
        let e = self.ensure_entry(surface_id);
        e.has_sub_pos = true;
        e.x = x;
        e.y = y;
    }

    /// Attach captured pending state to a surface's entry.
    pub fn set_state(&mut self, surface_id: u32, state_id: u32) {
        self.ensure_entry(surface_id).state_id = Some(state_id);
    }

    /// meta_wayland_transaction_add_dma_buf_source / add_drm_syncobj_source.
    ///
    /// STUB: real mutter installs GSources that fire when the buffer's fence
    /// signals. Here we just record the buffer id as an unmet dependency.
    pub fn add_buffer_dependency(&mut self, surface_id: u32, buffer_id: u32) {
        self.ensure_entry(surface_id)
            .buf_dependencies
            .push(buffer_id);
    }

    /// Mark a buffer ready across all entries (a fence signalled).
    pub fn buffer_ready(&mut self, buffer_id: u32) {
        for e in self.entries.values_mut() {
            e.buf_dependencies.retain(|b| *b != buffer_id);
        }
    }

    /// has_dependencies: any entry still waiting on a buffer.
    pub fn has_dependencies(&self) -> bool {
        self.entries
            .values()
            .any(|e| !e.buf_dependencies.is_empty())
    }

    pub fn surface_ids(&self) -> Vec<u32> {
        self.entries.keys().copied().collect()
    }

    /// meta_wayland_transaction_merge_into: absorb an older transaction's
    /// entries (older `from` is merged into newer `self`).
    pub fn merge_into(&mut self, from: &MetaWaylandTransaction) {
        for (sid, entry) in from.entries.iter() {
            let into = self.ensure_entry(*sid);
            entry.merge_into(into);
        }
    }
}

/// Owns transactions and applies them in committed order once ready
/// (the MetaWaylandCompositor's transaction bookkeeping).
pub struct TransactionManager {
    transactions: BTreeMap<u32, MetaWaylandTransaction>,
    next_id: AtomicU32,
    commit_counter: u64,
}

impl TransactionManager {
    pub fn new() -> Self {
        TransactionManager {
            transactions: BTreeMap::new(),
            next_id: AtomicU32::new(1),
            commit_counter: 0,
        }
    }

    /// meta_wayland_transaction_new.
    pub fn create(&mut self) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Release);
        self.transactions
            .insert(id, MetaWaylandTransaction::new(id));
        id
    }

    pub fn get(&self, id: u32) -> Option<&MetaWaylandTransaction> {
        self.transactions.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut MetaWaylandTransaction> {
        self.transactions.get_mut(&id)
    }

    /// meta_wayland_transaction_commit: assign a monotonic sequence.
    pub fn commit(&mut self, id: u32) -> bool {
        self.commit_counter += 1;
        let seq = self.commit_counter;
        match self.transactions.get_mut(&id) {
            Some(t) => {
                t.committed_sequence = seq;
                true
            }
            None => false,
        }
    }

    /// meta_wayland_transaction_maybe_apply: apply every committed, dependency-
    /// free transaction in ascending sequence order. Returns applied ids.
    ///
    /// STUB: does not enforce ancestor-before-descendant surface ordering
    /// beyond commit sequence.
    pub fn maybe_apply(&mut self) -> Vec<u32> {
        let mut ready: Vec<(u64, u32)> = self
            .transactions
            .values()
            .filter(|t| t.committed_sequence > 0 && !t.has_dependencies())
            .map(|t| (t.committed_sequence, t.id))
            .collect();
        ready.sort_unstable();

        let mut applied = Vec::new();
        for (_seq, id) in ready {
            self.transactions.remove(&id);
            applied.push(id);
        }
        applied
    }

    /// meta_wayland_transaction_free (explicit drop without applying).
    pub fn free(&mut self, id: u32) -> bool {
        self.transactions.remove(&id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_and_position() {
        let mut t = MetaWaylandTransaction::new(1);
        t.add_subsurface_position(10, 5, 6);
        let e = t.get_entry(10).unwrap();
        assert!(e.has_sub_pos);
        assert_eq!((e.x, e.y), (5, 6));
    }

    #[test]
    fn test_dependencies_gate_apply() {
        let mut mgr = TransactionManager::new();
        let id = mgr.create();
        mgr.get_mut(id).unwrap().add_buffer_dependency(10, 77);
        mgr.commit(id);
        // Still blocked on buffer 77.
        assert!(mgr.maybe_apply().is_empty());
        mgr.get_mut(id).unwrap().buffer_ready(77);
        assert_eq!(mgr.maybe_apply(), alloc::vec![id]);
    }

    #[test]
    fn test_apply_order_by_sequence() {
        let mut mgr = TransactionManager::new();
        let a = mgr.create();
        let b = mgr.create();
        mgr.commit(b);
        mgr.commit(a);
        // b committed first -> applied first.
        assert_eq!(mgr.maybe_apply(), alloc::vec![b, a]);
    }

    #[test]
    fn test_merge_into() {
        let mut newer = MetaWaylandTransaction::new(2);
        let mut older = MetaWaylandTransaction::new(1);
        older.add_subsurface_position(10, 1, 2);
        newer.merge_into(&older);
        assert!(newer.get_entry(10).unwrap().has_sub_pos);
    }
}
