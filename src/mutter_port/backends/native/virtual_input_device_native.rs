//! Virtual input device implementation for GNOME Mutter.
//!
//! Provides programmatic injection of input events (keyboard, mouse, touch, scroll)
//! for testing and accessibility features. Runs event handling on the input thread
//! via the seat implementation.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-virtual-input-device-native.h

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// Virtual input device implementation.
pub struct VirtualInputDeviceNative {
    /// Base slot index for touch events.
    pub slot_base: u32,
    /// Implementation state (opaque C handle).
    pub impl_state: *mut c_void,
}

impl VirtualInputDeviceNative {
    /// Create a new virtual input device.
    pub fn new() -> Self {
        VirtualInputDeviceNative {
            slot_base: 0,
            impl_state: core::ptr::null_mut(),
        }
    }
}

impl Default for VirtualInputDeviceNative {
    fn default() -> Self {
        Self::new()
    }
}
