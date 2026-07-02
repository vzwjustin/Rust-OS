//! Input Settings Native implementation for GNOME Mutter.
//!
//! Manages per-device input settings (send-events, acceleration, handedness, etc.)
//! for input devices via libinput. Runs on the input thread via the seat implementation.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-input-settings-native.h

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// Native input settings for device configuration.
pub struct InputSettingsNative {
    /// Reference to the seat implementation (opaque C handle).
    pub seat_impl: *mut c_void,
}

impl InputSettingsNative {
    /// Create a new native input settings handler.
    pub fn new() -> Self {
        InputSettingsNative {
            seat_impl: core::ptr::null_mut(),
        }
    }
}

impl Default for InputSettingsNative {
    fn default() -> Self {
        Self::new()
    }
}
