//! Pointer Constraint implementation for GNOME Mutter.
//!
//! Enforces pointer confinement and lock constraints in response to Wayland
//! pointer-constraints protocol requests. Uses regions and border geometry
//! to track and enforce movement limits.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-pointer-constraint-native.h

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// Point structure for origin coordinates.
#[derive(Debug, Clone, Copy)]
pub struct GraphenePoint {
    pub x: f32,
    pub y: f32,
}

/// Pointer constraint implementation for native backend.
pub struct PointerConstraintNative {
    /// Reference to parent constraint (opaque C handle).
    pub constraint: *mut c_void,
    /// Seat for event handling (opaque C handle).
    pub seat: *mut c_void,
    /// Constraint region (opaque C handle).
    pub region: *mut c_void,
    /// Origin point for constraint.
    pub origin: GraphenePoint,
    /// Minimum edge distance for boundaries.
    pub min_edge_distance: f64,
}

impl PointerConstraintNative {
    /// Create a new pointer constraint.
    pub fn new() -> Self {
        PointerConstraintNative {
            constraint: core::ptr::null_mut(),
            seat: core::ptr::null_mut(),
            region: core::ptr::null_mut(),
            origin: GraphenePoint { x: 0.0, y: 0.0 },
            min_edge_distance: 0.0,
        }
    }
}

impl Default for PointerConstraintNative {
    fn default() -> Self {
        Self::new()
    }
}
