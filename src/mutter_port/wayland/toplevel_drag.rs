//! Wayland Toplevel Drag module
//!
//! Handles window dragging via the xdg-toplevel-drag-v1 protocol for Wayland.
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-toplevel-drag.h

use core::ffi::c_void;

/// Represents an active toplevel (window) drag operation in progress.
pub struct MetaWaylandToplevelDrag {
    /// Protocol resource handle
    pub resource: Option<*mut c_void>,
    /// Data source providing drag data
    pub data_source: Option<*mut c_void>,
    /// Surface being dragged
    pub dragged_surface: Option<*mut c_void>,
    /// X offset of drag origin from surface
    pub x_offset: i32,
    /// Y offset of drag origin from surface
    pub y_offset: i32,
    /// Window drag operation handle
    pub window_drag: Option<*mut c_void>,
    /// Event handler for drag events
    pub handler: Option<*mut c_void>,
    /// Signal handler ID for window unmanaging
    pub window_unmanaging_handler_id: u64,
    /// Signal handler ID for window shown
    pub window_shown_handler_id: u64,
    /// Signal handler ID for drag ended
    pub drag_ended_handler_id: u64,
    /// Signal handler ID for source destroyed
    pub source_destroyed_handler_id: u64,
}

impl MetaWaylandToplevelDrag {
    /// Create a new toplevel drag instance
    pub fn new() -> Self {
        Self {
            resource: None,
            data_source: None,
            dragged_surface: None,
            x_offset: 0,
            y_offset: 0,
            window_drag: None,
            handler: None,
            window_unmanaging_handler_id: 0,
            window_shown_handler_id: 0,
            drag_ended_handler_id: 0,
            source_destroyed_handler_id: 0,
        }
    }

    /// Initialize xdg-toplevel-drag protocol
    /// ponytail: register protocol; real impl listens for drag requests
    pub fn init(_compositor: *mut c_void) {}

    /// Calculate origin bounds for dragged window
    /// ponytail: real impl computes geometry accounting for offsets
    pub fn calc_origin_for_dragged_window(&self, _bounds_out: *mut c_void) -> bool {
        false
    }

    /// End the toplevel drag operation
    /// ponytail: clean up resources and disconnect signals
    pub fn end(&mut self) {
        self.window_drag = None;
        self.handler = None;
        self.window_unmanaging_handler_id = 0;
        self.window_shown_handler_id = 0;
        self.drag_ended_handler_id = 0;
        self.source_destroyed_handler_id = 0;
    }
}

impl Default for MetaWaylandToplevelDrag {
    fn default() -> Self {
        Self::new()
    }
}
