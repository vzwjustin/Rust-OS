//! Wayland Tablet Pad Strip module
//!
//! Ported from: meta-wayland-tablet-pad-strip.c/h

use alloc::{string::String, vec::Vec, format};

pub struct MetaWaylandTabletPadStrip {
    pub pad: Option<*mut core::ffi::c_void>, // MetaWaylandTabletPad pointer
    pub group: Option<*mut core::ffi::c_void>, // MetaWaylandTabletPadGroup pointer
    pub resource_list: Vec<*mut core::ffi::c_void>,
    pub focus_resource_list: Vec<*mut core::ffi::c_void>,
    pub feedback: Option<String>,
}
