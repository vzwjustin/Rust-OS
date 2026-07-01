//! Barrier Native ported from GNOME Mutter's src/backends/
//!
//! Implements native pointer barriers for constraining cursor movement.
//! Handles barrier lifecycle and event signaling (hit/left).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-barrier-private.h

/// Opaque barrier implementation type.
pub struct MetaBarrierImpl {
    active: bool,
    /// Direction flags controlling which axes the barrier blocks.
    flags: MetaBarrierFlags,
}

/// Barrier event for cursor crossing.
pub struct MetaBarrierEvent;

/// Opaque backend type.
pub struct MetaBackend;

/// Opaque barrier type.
pub struct MetaBarrier;

/// Border type for barrier geometry.
pub struct MetaBorder;

/// Barrier direction flags, matching the XFixes `XFixesBarrierFlags`
/// values used by upstream Mutter (`meta_barrier_native.c`).
///
/// Each flag indicates a direction in which the pointer is allowed to
/// travel through the barrier. A barrier with no flags blocks movement
/// in every direction; combining flags relaxes the block for the
/// corresponding axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaBarrierNativeFlags {
    /// Pointer may travel in the positive X direction (rightward).
    PositiveX = 1,
    /// Pointer may travel in the positive Y direction (downward).
    PositiveY = 2,
    /// Pointer may travel in the negative X direction (leftward).
    NegativeX = 4,
    /// Pointer may travel in the negative Y direction (upward).
    NegativeY = 8,
}

impl MetaBarrierNativeFlags {
    /// Convert a flag variant to its bitmask value.
    pub const fn bits(self) -> u32 {
        self as u32
    }
}

/// Barrier flags (bitmask). Type alias + consts so values can be OR-ed.
pub type MetaBarrierFlags = u32;

/// No directions permitted (barrier blocks all movement).
pub const META_BARRIER_FLAG_NONE: MetaBarrierFlags = 0;
/// Positive X direction permitted.
pub const META_BARRIER_FLAG_POSITIVE_X: MetaBarrierFlags = MetaBarrierNativeFlags::PositiveX.bits();
/// Positive Y direction permitted.
pub const META_BARRIER_FLAG_POSITIVE_Y: MetaBarrierFlags = MetaBarrierNativeFlags::PositiveY.bits();
/// Negative X direction permitted.
pub const META_BARRIER_FLAG_NEGATIVE_X: MetaBarrierFlags = MetaBarrierNativeFlags::NegativeX.bits();
/// Negative Y direction permitted.
pub const META_BARRIER_FLAG_NEGATIVE_Y: MetaBarrierFlags = MetaBarrierNativeFlags::NegativeY.bits();

impl MetaBarrierImpl {
    /// Create a new barrier implementation (initially inactive, no flags).
    pub fn new() -> Self {
        Self {
            active: false,
            flags: META_BARRIER_FLAG_NONE,
        }
    }

    /// Activate the barrier.
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Check if barrier is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Set the direction flags for this barrier.
    pub fn set_flags(&mut self, flags: MetaBarrierFlags) {
        self.flags = flags;
    }

    /// Get the direction flags for this barrier.
    pub fn get_flags(&self) -> MetaBarrierFlags {
        self.flags
    }

    /// Check whether the barrier permits travel in the given direction.
    pub fn permits_direction(&self, flag: MetaBarrierNativeFlags) -> bool {
        (self.flags & flag.bits()) != 0
    }

    /// Release from barrier (move through it). Temporarily deactivates
    /// the barrier so the cursor can pass through.
    pub fn release(&mut self, _event: &MetaBarrierEvent) {
        self.active = false;
    }

    /// Destroy barrier implementation. Deactivates and cleans up state.
    pub fn destroy(&mut self) {
        self.active = false;
        self.flags = META_BARRIER_FLAG_NONE;
    }
}

impl Default for MetaBarrierImpl {
    fn default() -> Self {
        Self::new()
    }
}

// Signal emission helpers. Without a GObject signal system, these
// are no-op placeholders that document the signal emission contract.
// A full implementation would call g_signal_emit() on the barrier.

/// Emit the "hit" signal when the cursor hits a barrier.
pub fn meta_barrier_emit_hit_signal(_barrier: &MetaBarrier, _event: &MetaBarrierEvent) {
    // Signal emission requires GObject signal dispatch infrastructure.
}

/// Emit the "left" signal when the cursor leaves a barrier.
pub fn meta_barrier_emit_left_signal(_barrier: &MetaBarrier, _event: &MetaBarrierEvent) {
    // Signal emission requires GObject signal dispatch infrastructure.
}

/// Decrement the barrier event reference count.
/// Without GObject refcounting, this is a no-op.
pub fn meta_barrier_event_unref(_event: &MetaBarrierEvent) {
    // Refcount management requires GObject infrastructure.
}

// Barrier query helpers. Without barrier storage, these return defaults.

/// Get the backend associated with a barrier.
pub fn meta_barrier_get_backend(_barrier: &MetaBarrier) -> Option<&MetaBackend> {
    None
}

/// Get the border geometry of a barrier.
pub fn meta_barrier_get_border(_barrier: &MetaBarrier) -> Option<&MetaBorder> {
    None
}

/// Get the flags of a barrier.
pub fn meta_barrier_get_flags(_barrier: &MetaBarrier) -> u32 {
    META_BARRIER_FLAG_NONE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flag_values_match_xfixes() {
        // XFixes barrier flag bit assignments.
        assert_eq!(META_BARRIER_FLAG_POSITIVE_X, 1);
        assert_eq!(META_BARRIER_FLAG_POSITIVE_Y, 2);
        assert_eq!(META_BARRIER_FLAG_NEGATIVE_X, 4);
        assert_eq!(META_BARRIER_FLAG_NEGATIVE_Y, 8);
    }

    #[test]
    fn test_flag_or_combination() {
        let flags = META_BARRIER_FLAG_POSITIVE_X | META_BARRIER_FLAG_NEGATIVE_Y;
        assert_eq!(flags, 1 | 8);
    }

    #[test]
    fn test_barrier_flags_lifecycle() {
        let mut b = MetaBarrierImpl::new();
        assert_eq!(b.get_flags(), META_BARRIER_FLAG_NONE);
        b.set_flags(META_BARRIER_FLAG_POSITIVE_X | META_BARRIER_FLAG_NEGATIVE_X);
        assert!(b.permits_direction(MetaBarrierNativeFlags::PositiveX));
        assert!(b.permits_direction(MetaBarrierNativeFlags::NegativeX));
        assert!(!b.permits_direction(MetaBarrierNativeFlags::PositiveY));
        b.destroy();
        assert_eq!(b.get_flags(), META_BARRIER_FLAG_NONE);
    }
}
