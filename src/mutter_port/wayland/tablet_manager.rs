//! Wayland Tablet Manager — coordinates tablet input devices and sessions.
//!
//! Manages tablet (stylus) and tablet-pad input devices, seat associations,
//! and protocol resource tracking for the Wayland tablet protocol.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet-manager.h

use alloc::vec::Vec;

/// Manages tablet device registration and per-seat tablet sessions.
#[derive(Debug)]
pub struct MetaWaylandTabletManager {
    /// Pointer to the parent compositor.
    pub compositor: *mut core::ffi::c_void,
    /// Wayland display for resource tracking.
    pub wl_display: *mut core::ffi::c_void,
    /// List of protocol resources bound by clients.
    pub resource_list: Vec<*mut core::ffi::c_void>,
    /// Hash table mapping seats to tablet sessions (GHashTable*).
    pub seats: *mut core::ffi::c_void,
}

impl MetaWaylandTabletManager {
    /// Create a new tablet manager.
    pub fn new(
        compositor: *mut core::ffi::c_void,
        wl_display: *mut core::ffi::c_void,
    ) -> Self {
        Self {
            compositor,
            wl_display,
            resource_list: Vec::new(),
            seats: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaWaylandTabletManager {
    fn default() -> Self {
        Self::new(core::ptr::null_mut(), core::ptr::null_mut())
    }
}

/// Initialize tablet manager for the compositor.
///
/// Registers tablet_manager protocol and prepares device tracking.
pub fn meta_wayland_tablet_manager_init(_compositor: *mut core::ffi::c_void) {
    // TODO: tablet protocol registration
}

/// Finalize tablet manager — clean up resources and remove protocol.
///
/// Closes all tablet sessions and frees device tables.
pub fn meta_wayland_tablet_manager_finalize(_compositor: *mut core::ffi::c_void) {
    // TODO: cleanup tablet sessions and protocol
}
