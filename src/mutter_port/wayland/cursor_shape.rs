//! Wayland Cursor Shape protocol implementation.
//!
//! Ported from: meta-wayland-cursor-shape.c/h
//!
//! Implements the wp_cursor_shape_manager_v1 and wp_cursor_shape_device_v1 protocols,
//! allowing clients to request named cursor shapes instead of providing bitmap data.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-cursor-shape.h

use alloc::{string::String, vec::Vec};

/// Named cursor shape enumeration (mirrors wp_cursor_shape_device_v1 shape values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum WpCursorShape {
    // Core shapes
    DEFAULT = 1,
    CONTEXT_MENU = 2,
    HELP = 3,
    POINTER = 4,
    PROGRESS = 5,
    WAIT = 6,
    CELL = 7,
    CROSSHAIR = 8,
    TEXT = 9,
    VERTICAL_TEXT = 10,
    ALIAS = 11,
    COPY = 12,
    MOVE = 13,
    NO_DROP = 14,
    NOT_ALLOWED = 15,
    GRAB = 16,
    GRABBING = 17,
    // Resize/edge shapes
    E_RESIZE = 18,
    N_RESIZE = 19,
    NE_RESIZE = 20,
    NW_RESIZE = 21,
    S_RESIZE = 22,
    SE_RESIZE = 23,
    SW_RESIZE = 24,
    W_RESIZE = 25,
    EW_RESIZE = 26,
    NS_RESIZE = 27,
    NESW_RESIZE = 28,
    NWSE_RESIZE = 29,
    COL_RESIZE = 30,
    ROW_RESIZE = 31,
    // Zoom shapes
    ALL_SCROLL = 32,
    ZOOM_IN = 33,
    ZOOM_OUT = 34,
}

/// Cursor shape manager for a Wayland compositor.
///
/// Maintains the wp_cursor_shape_manager_v1 global resource and per-device
/// cursor shape state. Protocol I/O is TODO; this holds the data model.
#[derive(Debug)]
pub struct MetaWaylandCursorShape {
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
}

impl MetaWaylandCursorShape {
    pub fn new(compositor: *mut core::ffi::c_void) -> Self {
        MetaWaylandCursorShape {
            compositor: if compositor.is_null() { None } else { Some(compositor) },
        }
    }
}

impl Default for MetaWaylandCursorShape {
    fn default() -> Self {
        MetaWaylandCursorShape { compositor: None }
    }
}
