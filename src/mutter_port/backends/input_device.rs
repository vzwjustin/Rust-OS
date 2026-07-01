//! Input Device — ported from GNOME Mutter
//!
//! Base input device abstraction for keyboard, mouse, touchpad, and other input
//! devices. This module provides the core type hierarchy for device management.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-device.c

use core::ffi::c_void;

/// Base input device type. Carries reference to backend and optional Wacom device.
pub struct InputDevice {
    /// Reference to the backend (opaque).
    pub backend: *mut c_void,
    /// Optional Wacom device pointer (opaque WacomDevice, may be null).
    pub wacom_device: *mut c_void,
}

impl InputDevice {
    /// Create a new input device.
    pub fn new() -> Self {
        InputDevice {
            backend: core::ptr::null_mut(),
            wacom_device: core::ptr::null_mut(),
        }
    }
}

impl Default for InputDevice {
    fn default() -> Self {
        Self::new()
    }
}
