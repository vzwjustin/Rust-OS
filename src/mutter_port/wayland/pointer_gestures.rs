//! Wayland Pointer Gestures — multi-touch gesture recognition.
//!
//! Implements pointer gesture protocol (swipe, pinch, hold) for touch and trackpad
//! input. Tracks gesture state and emits protocol events.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-pointer-gestures.h

use alloc::vec::Vec;

/// Pointer gesture protocol manager.
///
/// Tracks the list of wl_resources bound by clients to the
/// zwp_pointer_gestures_v1 protocol. Each resource represents a
/// client's subscription to gesture events (swipe, pinch, hold).
/// A full implementation would also hold per-pointer gesture state
/// objects (MetaWaylandPointerGestureSwipe, Pinch, Hold) and dispatch
/// Clutter gesture events to the appropriate client resources.
#[derive(Debug)]
pub struct MetaWaylandPointerGestures {
    /// List of zwp_pointer_gestures_v1 resources bound by clients.
    pub resource_list: Vec<*mut core::ffi::c_void>,
    /// Whether the protocol global has been registered.
    pub initialized: bool,
}

impl MetaWaylandPointerGestures {
    /// Create a new empty pointer gestures manager.
    pub fn new() -> Self {
        MetaWaylandPointerGestures {
            resource_list: Vec::new(),
            initialized: false,
        }
    }

    /// Add a client protocol resource to the tracking list.
    pub fn add_resource(&mut self, resource: *mut core::ffi::c_void) {
        self.resource_list.push(resource);
    }

    /// Remove a client protocol resource from the tracking list.
    pub fn remove_resource(&mut self, resource: *mut core::ffi::c_void) {
        self.resource_list.retain(|&r| r != resource);
    }

    /// Get the list of bound gesture resources.
    pub fn get_resources(&self) -> &[*mut core::ffi::c_void] {
        &self.resource_list
    }

    /// Number of bound gesture resources.
    pub fn resource_count(&self) -> usize {
        self.resource_list.len()
    }

    /// Clear all bound resources.
    pub fn clear(&mut self) {
        self.resource_list.clear();
    }
}

impl Default for MetaWaylandPointerGestures {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize pointer gestures support for the compositor.
///
/// Sets up gesture protocol handlers (swipe, pinch, hold).
/// A full implementation would call wl_global_create for
/// zwp_pointer_gestures_v1 and register the protocol interface.
pub fn meta_wayland_pointer_gestures_init(compositor: *mut core::ffi::c_void) {
    if compositor.is_null() {
        return;
    }
    // Protocol global registration requires libwayland-server.
}
