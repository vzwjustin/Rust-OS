//! XWayland Surface — Wayland-native proxy for X11 windows.
//!
//! Represents a Wayland surface that bridges X11 window (XWayland) content.
//! Tracks actor surface state and X11 window association.
//!
//! Upstream header not found; minimal stub based on port pattern.

/// A Wayland surface wrapping an XWayland (X11) window.
#[derive(Debug)]
pub struct MetaXwaylandSurface {
    /// The underlying actor surface (rendering surface).
    pub actor_surface: *mut core::ffi::c_void,
}

impl MetaXwaylandSurface {
    /// Create a new XWayland surface.
    pub fn new(actor_surface: *mut core::ffi::c_void) -> Self {
        Self { actor_surface }
    }
}

impl Default for MetaXwaylandSurface {
    fn default() -> Self {
        Self::new(core::ptr::null_mut())
    }
}

/// Associate an XWayland surface with an X11 window.
///
/// Links the Wayland surface to the underlying X11 window for event routing.
/// X11 protocol I/O is left as TODO.
pub fn meta_xwayland_surface_associate_with_window(
    _xwayland_surface: *mut MetaXwaylandSurface,
    _window: *mut core::ffi::c_void,
) {
    // TODO: X11 window association, sync properties
}
