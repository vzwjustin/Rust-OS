//! Barrier Native ported from GNOME Mutter's src/backends/
//!
//! Implements native pointer barriers for constraining cursor movement.
//! Handles barrier lifecycle and event signaling (hit/left).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-barrier-private.h

/// Opaque barrier implementation type.
pub struct MetaBarrierImpl;

/// Barrier event for cursor crossing.
pub struct MetaBarrierEvent;

/// Opaque backend type.
pub struct MetaBackend;

/// Opaque barrier type.
pub struct MetaBarrier;

/// Border type for barrier geometry.
pub struct MetaBorder;

/// Barrier flags (bitmask). Type alias + consts so values can be OR-ed.
/// TODO: extract the concrete flag constants from upstream.
pub type MetaBarrierFlags = u32;

// Barrier implementation interface
impl MetaBarrierImpl {
    /// Check if barrier is currently active.
    pub fn is_active(&self) -> bool {
        // TODO: implement
        false
    }

    /// Release from barrier (move through it).
    pub fn release(&mut self, event: &MetaBarrierEvent) {
        // TODO: implement
    }

    /// Destroy barrier implementation.
    pub fn destroy(&mut self) {
        // TODO: implement
    }
}

// Signal emission helpers (TODO: wire up to actual signal dispatch)
pub fn meta_barrier_emit_hit_signal(barrier: &MetaBarrier, event: &MetaBarrierEvent) {
    // TODO: emit "hit" signal
}

pub fn meta_barrier_emit_left_signal(barrier: &MetaBarrier, event: &MetaBarrierEvent) {
    // TODO: emit "left" signal
}

pub fn meta_barrier_event_unref(_event: &MetaBarrierEvent) {
    // TODO: decrement refcount
}

// Barrier query helpers (TODO: wire up to actual barrier storage)
pub fn meta_barrier_get_backend(_barrier: &MetaBarrier) -> Option<&MetaBackend> {
    // TODO: return backend
    None
}

pub fn meta_barrier_get_border(_barrier: &MetaBarrier) -> Option<&MetaBorder> {
    // TODO: return border
    None
}

pub fn meta_barrier_get_flags(_barrier: &MetaBarrier) -> u32 {
    // TODO: return flags
    0
}
