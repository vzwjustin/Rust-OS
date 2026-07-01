//! Wayland Tablet Seat module
//!
//! Ported from: meta-wayland-tablet-seat.c/h

use alloc::{string::String, vec::Vec, format};

pub struct MetaWaylandTabletSeat {
    pub manager: Option<*mut core::ffi::c_void>, // MetaWaylandTabletManager pointer
    pub seat: Option<*mut core::ffi::c_void>, // MetaWaylandSeat pointer
    pub clutter_seat: Option<*mut core::ffi::c_void>, // ClutterSeat pointer
    pub resource_list: Vec<*mut core::ffi::c_void>,
    pub tablets: Option<*mut core::ffi::c_void>, // GHashTable of tablets
    pub tools: Option<*mut core::ffi::c_void>, // GHashTable of tools
    pub pads: Option<*mut core::ffi::c_void>, // GHashTable of pads
}
