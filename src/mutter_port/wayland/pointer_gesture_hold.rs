//! Wayland Pointer Gesture Hold module
//!
//! Handles pointer hold gestures (multi-finger stationary hold).
//! Forwards ClutterEvent-based gesture data to Wayland clients.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-pointer-gesture-hold.h

/// Pointer gesture hold handler.
pub struct MetaWaylandPointerGestureHold;

impl MetaWaylandPointerGestureHold {
    /// Handle a pointer gesture hold event from clutter.
    pub fn handle_event(
        _pointer: *mut core::ffi::c_void,
        _event: *const core::ffi::c_void,
    ) -> bool {
        // TODO: translate ClutterEvent to wp_pointer_gesture_hold events
        false
    }

    /// Create a new resource for gesture hold protocol.
    pub fn create_new_resource(
        _pointer: *mut core::ffi::c_void,
        _client: *mut core::ffi::c_void,
        _gestures_resource: *mut core::ffi::c_void,
        _id: u32,
    ) {
        // TODO: bind new wl_resource to gestures interface
    }

    /// Cancel an ongoing hold gesture.
    pub fn cancel(_pointer: *mut core::ffi::c_void, _serial: u32) {
        // TODO: emit cancel event to clients
    }
}
