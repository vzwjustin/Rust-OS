//! Wayland XWayland module
//!
//! Manages the Xwayland server subprocess and X11 window integration with Wayland.
//! Handles display number assignment, window-to-surface mapping, and X11-Wayland window property bridging.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-xwayland.h

/// Override the XWayland display number.
///
/// Sets the X display number (e.g., :1, :99) for the Xwayland server.
/// Must be called before server initialization.
///
/// ponytail: real impl manages X display numbering and Xwayland server state
pub fn meta_xwayland_override_display_number(_number: i32) {}

/// Handle a wl_surface ID for an X11 window.
///
/// Associates an X11 window with its corresponding Wayland surface using the
/// surface ID retrieved via the _NET_WAYLAND_SURFACE_ID property.
///
/// ponytail: real impl maps X11 window to Wayland surface
pub fn meta_xwayland_handle_wl_surface_id(_window: *mut core::ffi::c_void, _surface_id: u32) {}

/// Associate an X11 window with a Wayland surface.
///
/// Creates the bidirectional mapping between an X11 window and a Wayland surface,
/// enabling property and event synchronization.
///
/// ponytail: real impl creates bidirectional mapping and signal connections
pub fn meta_xwayland_associate_window_with_surface(
    _window: *mut core::ffi::c_void,
    _surface: *mut core::ffi::c_void,
) {
}
