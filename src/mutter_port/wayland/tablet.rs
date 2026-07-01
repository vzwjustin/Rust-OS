//! Wayland Tablet module
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet.h
//!
//! Represents a tablet input device in Wayland, modeling physical stylus/pen hardware.
//! Manages resource binding and sprite state; protocol I/O is TODO.

/// A tablet input device (stylus, pen) connected to the seat.
/// The C struct uses `struct wl_list` for the resource list; here we use opaque pointers.
pub struct MetaWaylandTablet {
    /// Parent tablet seat managing this tablet.
    pub tablet_seat: *mut core::ffi::c_void,
    /// Clutter input device backing this tablet.
    pub device: *mut core::ffi::c_void,
    /// Sprite (cursor image) for this tablet; may be null.
    pub sprite: *mut core::ffi::c_void,
    /// Linked list of Wayland resources (opaque).
    pub resource_list: *mut core::ffi::c_void,
    /// Current surface receiving tablet events, if any.
    pub current: *mut core::ffi::c_void,
}

impl MetaWaylandTablet {
    /// Create a new tablet bound to a device and tablet seat.
    /// TODO: Clutter device introspection (vendor ID, product ID, etc.).
    pub fn new() -> Self {
        Self {
            tablet_seat: core::ptr::null_mut(),
            device: core::ptr::null_mut(),
            sprite: core::ptr::null_mut(),
            resource_list: core::ptr::null_mut(),
            current: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaWaylandTablet {
    fn default() -> Self {
        Self::new()
    }
}
