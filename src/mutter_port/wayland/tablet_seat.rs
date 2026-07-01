//! Wayland Tablet Seat module
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet-seat.h
//!
//! Manages all tablet input devices for a given Wayland seat.
//! Holds collections of tablets, tools, and pads; protocol I/O and resource binding are TODO.

/// Manages tablet devices (stylus pens) for a Wayland seat.
/// The C implementation uses `GHashTable` for device collections and `struct wl_list` for resources.
pub struct MetaWaylandTabletSeat {
    /// Parent tablet manager.
    pub manager: *mut core::ffi::c_void,
    /// Wayland seat this tablet seat belongs to.
    pub seat: *mut core::ffi::c_void,
    /// Clutter seat providing input device enumeration.
    pub clutter_seat: *mut core::ffi::c_void,
    /// Linked list of Wayland resources (opaque).
    pub resource_list: *mut core::ffi::c_void,
    /// Hash table mapping ClutterInputDevice* to MetaWaylandTablet*.
    pub tablets: *mut core::ffi::c_void,
    /// Hash table mapping ClutterInputDevice* to MetaWaylandTabletTool*.
    pub tools: *mut core::ffi::c_void,
    /// Hash table mapping ClutterInputDevice* to MetaWaylandTabletPad*.
    pub pads: *mut core::ffi::c_void,
}

impl MetaWaylandTabletSeat {
    /// Create a new tablet seat for the given manager and Wayland seat.
    /// TODO: hash table allocation and resource list initialization.
    pub fn new() -> Self {
        Self {
            manager: core::ptr::null_mut(),
            seat: core::ptr::null_mut(),
            clutter_seat: core::ptr::null_mut(),
            resource_list: core::ptr::null_mut(),
            tablets: core::ptr::null_mut(),
            tools: core::ptr::null_mut(),
            pads: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaWaylandTabletSeat {
    fn default() -> Self {
        Self::new()
    }
}
