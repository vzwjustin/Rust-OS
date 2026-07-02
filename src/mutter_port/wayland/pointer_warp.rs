//! Wayland Pointer Warp module
//!
//! Manages pointer warping (cursor repositioning) for Wayland seats.
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-pointer-warp.h

use alloc::boxed::Box;
use core::ffi::c_void;

/// Pointer warp handler for programmatic cursor movement within a Wayland seat.
pub struct MetaWaylandPointerWarp {
    /// Associated Wayland seat
    pub seat: Option<*mut c_void>,
}

impl MetaWaylandPointerWarp {
    /// Create a new pointer warp for a wayland seat
    /// ponytail: register protocol if needed; stub just allocates handle
    pub fn new(seat: *mut c_void) -> Option<*mut c_void> {
        let warp = Box::new(MetaWaylandPointerWarp { seat: Some(seat) });
        Some(Box::into_raw(warp) as *mut c_void)
    }

    /// Destroy the pointer warp
    /// ponytail: cleanup unregisters from seat; stub just deallocates
    pub fn destroy(pointer_warp: *mut c_void) {
        if !pointer_warp.is_null() {
            unsafe {
                let _ = Box::from_raw(pointer_warp as *mut MetaWaylandPointerWarp);
            }
        }
    }
}

impl Default for MetaWaylandPointerWarp {
    fn default() -> Self {
        Self { seat: None }
    }
}
