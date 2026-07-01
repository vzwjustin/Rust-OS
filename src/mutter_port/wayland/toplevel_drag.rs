//! Wayland Toplevel Drag module
//!
//! Ported from: meta-wayland-toplevel-drag.c/h

use alloc::{string::String, vec::Vec, format};

pub struct MetaWaylandToplevelDrag {
    pub resource: Option<*mut core::ffi::c_void>, // wl_resource pointer
    pub data_source: Option<*mut core::ffi::c_void>, // MetaWaylandDataSource pointer
    pub dragged_surface: Option<*mut core::ffi::c_void>, // MetaWaylandSurface pointer
    pub x_offset: i32,
    pub y_offset: i32,
    pub window_drag: Option<*mut core::ffi::c_void>, // MetaWindowDrag pointer
}
