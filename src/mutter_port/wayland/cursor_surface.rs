//! Wayland Cursor Surface module
//!
//! Ported from: meta-wayland-cursor-surface.c/h

use alloc::{string::String, vec::Vec, format};

pub struct MetaWaylandCursorSurface {
    pub cursor: Option<*mut core::ffi::c_void>, // ClutterCursor pointer
    pub hotspot_x: i32,
    pub hotspot_y: i32,
    pub renderer: Option<*mut core::ffi::c_void>, // MetaCursorRenderer pointer
}

impl MetaWaylandCursorSurface {
    /// Get the cursor for this cursor surface
    /// TODO: port logic from meta_wayland_cursor_surface_get_cursor
    pub fn get_cursor(&self) -> Option<*mut core::ffi::c_void> {
        self.cursor
    }

    /// Set the hotspot coordinates for the cursor
    /// TODO: port logic from meta_wayland_cursor_surface_set_hotspot
    pub fn set_hotspot(&mut self, hotspot_x: i32, hotspot_y: i32) {
        self.hotspot_x = hotspot_x;
        self.hotspot_y = hotspot_y;
    }

    /// Set the cursor renderer for this surface
    /// TODO: port logic from meta_wayland_cursor_surface_set_renderer
    pub fn set_renderer(&mut self, renderer: Option<*mut core::ffi::c_void>) {
        self.renderer = renderer;
    }
}
