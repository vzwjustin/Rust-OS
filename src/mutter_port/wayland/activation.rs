//! Wayland Activation module
//!
//! Ported from: meta-wayland-activation.c/h

use alloc::{string::String, vec::Vec, format};

pub struct MetaXdgActivationToken {
    pub surface: Option<*mut core::ffi::c_void>, // MetaWaylandSurface pointer
    pub seat: Option<*mut core::ffi::c_void>,    // MetaWaylandSeat pointer
    pub activation: Option<*mut core::ffi::c_void>, // MetaWaylandActivation pointer
    pub sequence: Option<*mut core::ffi::c_void>,   // MetaStartupSequence pointer
    pub app_id: Option<String>,
    pub token: Option<String>,
    pub serial: u32,
    pub committed: bool,
}

pub struct MetaWaylandActivation {
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
    pub resource_list: Vec<*mut core::ffi::c_void>,
    pub token_list: Vec<*mut core::ffi::c_void>,
    pub tokens: Option<*mut core::ffi::c_void>, // GHashTable
    pub pending_activations: Option<*mut core::ffi::c_void>, // GHashTable
}

impl MetaWaylandActivation {
    /// Initialize wayland activation support for the compositor
    /// TODO: port logic from meta_wayland_activation_init
    pub fn init(_compositor: *mut core::ffi::c_void) {
        // TODO: implement
    }

    /// Finalize wayland activation support for the compositor
    /// TODO: port logic from meta_wayland_activation_finalize
    pub fn finalize(_compositor: *mut core::ffi::c_void) {
        // TODO: implement
    }
}
