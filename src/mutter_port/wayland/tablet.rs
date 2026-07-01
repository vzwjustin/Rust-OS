//! Wayland Tablet module
//!
//! Ported from: meta-wayland-tablet.c/h

use alloc::{string::String, vec::Vec, format};

pub struct MetaWaylandTablet {
    pub tablet_seat: Option<*mut core::ffi::c_void>, // MetaWaylandTabletSeat pointer
    pub device: Option<*mut core::ffi::c_void>, // ClutterInputDevice pointer
    pub sprite: Option<*mut core::ffi::c_void>, // ClutterSprite pointer
    pub resource_list: Vec<*mut core::ffi::c_void>,
    pub current: Option<*mut core::ffi::c_void>, // MetaWaylandSurface pointer
}
