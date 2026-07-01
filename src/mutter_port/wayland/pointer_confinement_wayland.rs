//! Wayland Pointer Confinement module
//!
//! Implements pointer constraint handling for fullscreen/locked pointer protocols.
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-pointer-confinement-wayland.h

use alloc::boxed::Box;
use core::ffi::c_void;

/// Wayland-based pointer confinement wraps a pointer constraint into a confinement region.
pub struct MetaPointerConfinementWayland {
    /// Associated Wayland pointer constraint
    pub constraint: Option<*mut c_void>,
    /// Parent pointer constraint object
    pub parent_constraint: Option<*mut c_void>,
}

impl MetaPointerConfinementWayland {
    /// Create a new pointer confinement from a wayland constraint
    /// ponytail: initialize confinement region from constraint; real impl calculates bounds
    pub fn new(constraint: *mut c_void) -> Option<*mut c_void> {
        let confinement = Box::new(MetaPointerConfinementWayland {
            constraint: Some(constraint),
            parent_constraint: None,
        });
        Some(Box::into_raw(confinement) as *mut c_void)
    }

    /// Get the underlying wayland pointer constraint
    pub fn get_wayland_pointer_constraint(&self) -> Option<*mut c_void> {
        self.constraint
    }

    /// Enable the pointer confinement
    /// ponytail: apply constraint; real impl restricts pointer movement
    pub fn enable(&mut self) {}

    /// Disable the pointer confinement
    /// ponytail: release constraint; real impl restores free pointer movement
    pub fn disable(&mut self) {}
}

impl Default for MetaPointerConfinementWayland {
    fn default() -> Self {
        Self {
            constraint: None,
            parent_constraint: None,
        }
    }
}
