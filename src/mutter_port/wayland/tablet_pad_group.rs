//! Wayland Tablet Pad Group module
//!
//! Ported from: meta-wayland-tablet-pad-group.c/h

use alloc::{string::String, vec::Vec, format};

pub struct MetaWaylandTabletPadGroup {
    pub pad: Option<*mut core::ffi::c_void>, // MetaWaylandTabletPad pointer
    pub n_modes: u32,
    pub current_mode: u32,
    pub resource_list: Vec<*mut core::ffi::c_void>,
    pub focus_resource_list: Vec<*mut core::ffi::c_void>,
    pub mode_switch_serial: u32,
    pub strips: Vec<*mut core::ffi::c_void>, // GList of strips
}
