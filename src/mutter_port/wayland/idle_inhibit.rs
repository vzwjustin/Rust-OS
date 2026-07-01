//! Wayland Idle Inhibit module
//!
//! Implements idle_inhibit_v1 protocol to prevent screen blanking
//! when fullscreen media (video, games) is active.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-idle-inhibit.h

/// Manages idle inhibition state for the Wayland compositor.
/// Tracks inhibitor objects that suppress screen blanking.
pub struct MetaWaylandIdleInhibit {
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
}

impl MetaWaylandIdleInhibit {
    /// Create a new idle inhibit handler (stub).
    pub fn new() -> Self {
        MetaWaylandIdleInhibit {
            compositor: None,
        }
    }

    /// Initialize idle inhibit support for the compositor.
    /// TODO: register idle_inhibit_v1 protocol
    pub fn init(_compositor: *mut core::ffi::c_void) -> bool {
        false
    }
}

impl Default for MetaWaylandIdleInhibit {
    fn default() -> Self {
        Self::new()
    }
}
