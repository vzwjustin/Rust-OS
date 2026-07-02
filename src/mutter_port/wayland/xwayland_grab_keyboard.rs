//! Wayland XWayland Grab Keyboard module
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-xwayland-grab-keyboard.h
//!
//! Manages XWayland keyboard grab protocol for X11 clients running under Wayland.
//! Tracks active keyboard grabs per X11 window.

/// Tracks an active XWayland keyboard grab.
pub struct MetaXwaylandKeyboardActiveGrab {
    /// The X11 window that holds the grab.
    pub xwindow: u64,
    /// Whether the grab is currently active.
    pub active: bool,
    /// The compositor instance (opaque).
    pub compositor: *mut core::ffi::c_void,
}

impl MetaXwaylandKeyboardActiveGrab {
    /// Create a new keyboard grab tracker.
    pub fn new() -> Self {
        Self {
            xwindow: 0,
            active: false,
            compositor: core::ptr::null_mut(),
        }
    }

    /// Initialize XWayland keyboard grab protocol support for the
    /// compositor. A full implementation would register the
    /// xwayland_keyboard_grab_manager_v1 global. Returns true on success.
    pub fn init(compositor: *mut core::ffi::c_void) -> bool {
        // Without libwayland, we can't register the protocol global.
        // Record the compositor pointer for future use.
        let _ = compositor;
        false
    }

    /// Activate a keyboard grab for the given X11 window.
    pub fn activate(&mut self, xwindow: u64) {
        self.xwindow = xwindow;
        self.active = true;
    }

    /// Deactivate the current keyboard grab.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.xwindow = 0;
    }

    /// Whether a grab is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get the X11 window holding the grab.
    pub fn get_xwindow(&self) -> u64 {
        self.xwindow
    }
}

impl Default for MetaXwaylandKeyboardActiveGrab {
    fn default() -> Self {
        Self::new()
    }
}
