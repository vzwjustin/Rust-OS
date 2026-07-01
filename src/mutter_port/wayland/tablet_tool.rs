//! Wayland Tablet Tool — stylus and tablet input tool abstraction.
//!
//! Represents a stylus or tablet tool with its capabilities (pressure, tilt, etc),
//! resource bindings, and event handling. Manages focus and grab state per client.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet-tool.h

use alloc::boxed::Box;

/// A tablet tool (stylus, eraser, pen) with per-client resources.
#[derive(Debug)]
pub struct MetaWaylandTabletTool {
    /// Seat this tool is bound to.
    pub tablet_seat: *mut core::ffi::c_void,
    /// Device tool (ClutterInputDeviceTool) backing this tool.
    pub device_tool: *mut core::ffi::c_void,
    /// Whether this tool currently has an active grab.
    pub has_grab: bool,
    /// Current pressure (0-65535, 0 = no contact).
    pub pressure: u32,
    /// Current tilt X (-65535 to 65535).
    pub tilt_x: i32,
    /// Current tilt Y (-65535 to 65535).
    pub tilt_y: i32,
}

impl MetaWaylandTabletTool {
    /// Create a new tablet tool bound to a seat.
    pub fn new(tablet_seat: *mut core::ffi::c_void, device_tool: *mut core::ffi::c_void) -> Self {
        Self {
            tablet_seat,
            device_tool,
            has_grab: false,
            pressure: 0,
            tilt_x: 0,
            tilt_y: 0,
        }
    }
}

impl Default for MetaWaylandTabletTool {
    fn default() -> Self {
        Self {
            tablet_seat: core::ptr::null_mut(),
            device_tool: core::ptr::null_mut(),
            has_grab: false,
            pressure: 0,
            tilt_x: 0,
            tilt_y: 0,
        }
    }
}

/// Create a new tablet tool. Allocates on the heap and returns a raw pointer.
pub fn meta_wayland_tablet_tool_new(
    tablet_seat: *mut core::ffi::c_void,
    device_tool: *mut core::ffi::c_void,
) -> *mut MetaWaylandTabletTool {
    Box::into_raw(Box::new(MetaWaylandTabletTool::new(
        tablet_seat,
        device_tool,
    )))
}

/// Free a tablet tool and its resources. Drops the heap-allocated tool.
pub fn meta_wayland_tablet_tool_free(tool: *mut MetaWaylandTabletTool) {
    if !tool.is_null() {
        unsafe {
            drop(Box::from_raw(tool));
        }
    }
}

/// Create a new protocol resource for a client. Without libwayland,
/// returns null. A full implementation would allocate a wl_resource
/// and bind it to the tablet_tool interface.
pub fn meta_wayland_tablet_tool_create_new_resource(
    _tool: *mut MetaWaylandTabletTool,
    _client: *mut core::ffi::c_void,
    _seat_resource: *mut core::ffi::c_void,
    _id: u32,
) -> *mut core::ffi::c_void {
    core::ptr::null_mut()
}

/// Look up an existing resource for a client. Without a resource hash
/// table, returns null. A full implementation would look up the
/// client's wl_resource from a hash table.
pub fn meta_wayland_tablet_tool_lookup_resource(
    _tool: *mut MetaWaylandTabletTool,
    _client: *mut core::ffi::c_void,
) -> *mut core::ffi::c_void {
    core::ptr::null_mut()
}

/// Update tool state from a Clutter event. A full implementation would
/// extract pressure, tilt, distance from the Clutter event. Without
/// Clutter event introspection, this is a no-op.
pub fn meta_wayland_tablet_tool_update(
    tool: *mut MetaWaylandTabletTool,
    _event: *const core::ffi::c_void,
) {
    if tool.is_null() {
        return;
    }
    // Event property extraction would update pressure, tilt_x, tilt_y.
}

/// Handle an input event for this tool. Returns true if the event was
/// consumed by an active grab, false otherwise.
pub fn meta_wayland_tablet_tool_handle_event(
    tool: *mut MetaWaylandTabletTool,
    _event: *const core::ffi::c_void,
) -> bool {
    if tool.is_null() {
        return false;
    }
    // SAFETY: The caller guarantees `tool` is a valid pointer from
    // meta_wayland_tablet_tool_new.
    let t = unsafe { &mut *tool };
    t.has_grab
}
