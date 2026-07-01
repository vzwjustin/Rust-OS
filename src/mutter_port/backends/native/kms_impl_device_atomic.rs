//! KMS atomic modeset implementation for a single device.
//!
//! Handles atomic KMS updates using DRM_IOCTL_MODE_ATOMIC and
//! property-based configuration. Requires DRM_CLIENT_CAP_ATOMIC capability.
//! Maintains device FD and atomic property state machine.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-kms-impl-device-atomic.c

use alloc::{boxed::Box, string::String, vec::Vec};

/// Atomic KMS implementation for a device
pub struct MetaKmsImplDeviceAtomic {
    /// Device file descriptor for DRM ioctls
    pub device_fd: i32,
    /// Atomic property state (opaque)
    pub atomic_state: *mut core::ffi::c_void,
    /// Pending atomic update blob (opaque)
    pub pending_update: *mut core::ffi::c_void,
}

impl MetaKmsImplDeviceAtomic {
    /// Create atomic implementation for a device
    pub fn new() -> Self {
        MetaKmsImplDeviceAtomic {
            device_fd: -1,
            atomic_state: core::ptr::null_mut(),
            pending_update: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaKmsImplDeviceAtomic {
    fn default() -> Self {
        Self::new()
    }
}
