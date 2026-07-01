//! Private KMS subsystem internals.
//!
//! Internal types and state management for the KMS subsystem,
//! including device list, resource tracking, and
//! update coordination across devices.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-kms-private.c

use alloc::{boxed::Box, string::String, vec::Vec};

/// Private KMS state for the subsystem
pub struct MetaKmsPrivate {
    /// List of KMS devices (opaque)
    pub devices: *mut core::ffi::c_void,
    /// Device resource pool (opaque)
    pub device_pool: *mut core::ffi::c_void,
    /// Pending updates queue (opaque)
    pub pending_updates: *mut core::ffi::c_void,
    /// Update sequence counter
    pub update_sequence: u64,
    /// Main thread context (opaque)
    pub main_context: *mut core::ffi::c_void,
    /// Number of active devices
    pub device_count: u32,
}

impl MetaKmsPrivate {
    /// Create private KMS state
    pub fn new() -> Self {
        MetaKmsPrivate {
            devices: core::ptr::null_mut(),
            device_pool: core::ptr::null_mut(),
            pending_updates: core::ptr::null_mut(),
            update_sequence: 0,
            main_context: core::ptr::null_mut(),
            device_count: 0,
        }
    }
}

impl Default for MetaKmsPrivate {
    fn default() -> Self {
        Self::new()
    }
}
