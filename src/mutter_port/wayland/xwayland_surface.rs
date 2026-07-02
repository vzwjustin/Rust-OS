//! XWayland Surface — Wayland-native proxy for X11 windows.
//!
//! Represents a Wayland surface that bridges X11 window (XWayland) content.
//! Tracks actor surface state and X11 window association.

/// A Wayland surface wrapping an XWayland (X11) window.
#[derive(Debug)]
pub struct MetaXwaylandSurface {
    /// The underlying actor surface (rendering surface).
    pub actor_surface: *mut core::ffi::c_void,
    /// Associated X11 window ID (0 if not yet associated).
    pub xwindow: u64,
    /// Whether the surface is currently mapped.
    pub mapped: bool,
}

impl MetaXwaylandSurface {
    /// Create a new XWayland surface.
    pub fn new(actor_surface: *mut core::ffi::c_void) -> Self {
        Self {
            actor_surface,
            xwindow: 0,
            mapped: false,
        }
    }

    /// Check if this surface is associated with an X11 window.
    pub fn is_associated(&self) -> bool {
        self.xwindow != 0
    }

    /// Mark the surface as mapped.
    pub fn set_mapped(&mut self, mapped: bool) {
        self.mapped = mapped;
    }
}

impl Default for MetaXwaylandSurface {
    fn default() -> Self {
        Self::new(core::ptr::null_mut())
    }
}

/// Associate an XWayland surface with an X11 window. Records the
/// X11 window ID on the surface for event routing. A full
/// implementation would sync X11 window properties (WM_NAME,
/// _NET_WM_STATE, etc.) to the Wayland surface.
pub fn meta_xwayland_surface_associate_with_window(
    xwayland_surface: *mut MetaXwaylandSurface,
    window: *mut core::ffi::c_void,
) {
    if xwayland_surface.is_null() {
        return;
    }
    // SAFETY: caller guarantees xwayland_surface is valid.
    let surface = unsafe { &mut *xwayland_surface };
    // Store the X11 window pointer as an ID. A full implementation
    // would extract the actual X window ID and sync properties.
    surface.xwindow = window as u64;
}
