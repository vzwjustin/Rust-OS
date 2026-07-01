//! Wayland Tablet Tool — stylus and tablet input tool abstraction.
//!
//! Represents a stylus or tablet tool with its capabilities (pressure, tilt, etc),
//! resource bindings, and event handling. Manages focus and grab state per client.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet-tool.h

/// A tablet tool (stylus, eraser, pen) with per-client resources.
#[derive(Debug)]
pub struct MetaWaylandTabletTool {
    /// Seat this tool is bound to.
    pub tablet_seat: *mut core::ffi::c_void,
    /// Device tool (ClutterInputDeviceTool) backing this tool.
    pub device_tool: *mut core::ffi::c_void,
}

impl MetaWaylandTabletTool {
    /// Create a new tablet tool bound to a seat.
    pub fn new(
        tablet_seat: *mut core::ffi::c_void,
        device_tool: *mut core::ffi::c_void,
    ) -> Self {
        Self {
            tablet_seat,
            device_tool,
        }
    }
}

impl Default for MetaWaylandTabletTool {
    fn default() -> Self {
        Self {
            tablet_seat: core::ptr::null_mut(),
            device_tool: core::ptr::null_mut(),
        }
    }
}

/// Create a new tablet tool.
pub fn meta_wayland_tablet_tool_new(
    _tablet_seat: *mut core::ffi::c_void,
    _device_tool: *mut core::ffi::c_void,
) -> *mut MetaWaylandTabletTool {
    // TODO: allocate and initialize tool
    core::ptr::null_mut()
}

/// Free a tablet tool and its resources.
pub fn meta_wayland_tablet_tool_free(_tool: *mut MetaWaylandTabletTool) {
    // TODO: cleanup resources
}

/// Create a new protocol resource for a client.
///
/// Binds a tablet_tool object for the given client and seat.
pub fn meta_wayland_tablet_tool_create_new_resource(
    _tool: *mut MetaWaylandTabletTool,
    _client: *mut core::ffi::c_void,
    _seat_resource: *mut core::ffi::c_void,
    _id: u32,
) -> *mut core::ffi::c_void {
    // TODO: wl_resource allocation + protocol binding
    core::ptr::null_mut()
}

/// Look up an existing resource for a client.
pub fn meta_wayland_tablet_tool_lookup_resource(
    _tool: *mut MetaWaylandTabletTool,
    _client: *mut core::ffi::c_void,
) -> *mut core::ffi::c_void {
    // TODO: hash table lookup
    core::ptr::null_mut()
}

/// Update tool state from a Clutter event.
///
/// Extracts pressure, tilt, distance from input event. Event dispatch is TODO.
pub fn meta_wayland_tablet_tool_update(
    _tool: *mut MetaWaylandTabletTool,
    _event: *const core::ffi::c_void,
) {
    // TODO: extract event properties, update state
}

/// Handle an input event for this tool.
///
/// Returns true if event was consumed by a grab, false otherwise.
pub fn meta_wayland_tablet_tool_handle_event(
    _tool: *mut MetaWaylandTabletTool,
    _event: *const core::ffi::c_void,
) -> bool {
    // TODO: grab tracking, event delivery
    false
}
