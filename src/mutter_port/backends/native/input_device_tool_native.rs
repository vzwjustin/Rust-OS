//! Input Device Tool Native — tablet tool representation.
//!
//! Represents a tablet stylus or eraser tool with pressure curve, button mapping,
//! and calibration data.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-input-device-tool-native.c

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

pub struct InputDeviceToolNative {
    /// Libinput tablet tool (opaque pointer to libinput_tablet_tool)
    pub tool: *mut c_void,
    /// Button mapping (opaque pointer to GHashTable)
    pub button_map: *mut c_void,
    /// Pressure curve control points [2] (8 bytes: 2 x f32)
    pub pressure_curve: [u32; 2],
    /// Bezier curve calibration (opaque pointer to MetaBezier)
    pub bezier: *mut c_void,
}

impl InputDeviceToolNative {
    pub fn new() -> Self {
        InputDeviceToolNative {
            tool: core::ptr::null_mut(),
            button_map: core::ptr::null_mut(),
            pressure_curve: [0; 2],
            bezier: core::ptr::null_mut(),
        }
    }
}

impl Default for InputDeviceToolNative {
    fn default() -> Self {
        Self::new()
    }
}
