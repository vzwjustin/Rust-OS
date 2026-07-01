//! Wayland Tablet Pad module
//!
//! Ported from: meta-wayland-tablet-pad.c/h

use alloc::{string::String, vec::Vec, format};

pub struct MetaWaylandTabletPad {
    pub tablet_seat: Option<*mut core::ffi::c_void>, // MetaWaylandTabletSeat pointer
    pub device: Option<*mut core::ffi::c_void>, // ClutterInputDevice pointer
    pub resource_list: Vec<*mut core::ffi::c_void>,
    pub focus_resource_list: Vec<*mut core::ffi::c_void>,
    pub focus_surface: Option<*mut core::ffi::c_void>, // MetaWaylandSurface pointer
    pub focus_serial: u32,
}
