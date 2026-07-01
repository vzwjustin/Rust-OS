//! Wayland Pointer Lock Wayland module
//!
//! Ported from: meta-pointer-lock-wayland.c/h

use alloc::{string::String, vec::Vec, format};

pub struct MetaPointerLockWayland {
    pub confinement: Option<*mut core::ffi::c_void>, // MetaPointerConfinementWayland pointer
}

impl MetaPointerLockWayland {
    /// Create a new pointer lock from a wayland constraint
    /// TODO: port logic from meta_pointer_lock_wayland_new
    pub fn new(
        _constraint: *mut core::ffi::c_void,
    ) -> Option<*mut core::ffi::c_void> {
        // TODO: implement
        None
    }
}
