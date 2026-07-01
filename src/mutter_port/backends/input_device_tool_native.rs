//! Input Device Tool Native — ported from GNOME Mutter
//!
//! Stylus and tablet tool support. Manages pressure curves, button mappings,
//! and tool-specific settings for pen and stylus devices.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-input-device-tool-native.h

use alloc::collections::BTreeMap;
use core::ffi::c_void;

pub const N_PRESSURECURVE_POINTS: usize = 256;

/// Point in 2D space for pressure curve control.
#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

/// Native tablet tool representation.
pub struct InputDeviceToolNative {
    /// libinput tablet tool pointer (opaque).
    pub tool: *mut c_void,
    /// Button to action mapping.
    pub button_map: BTreeMap<u32, u32>,
    /// Pressure curve control points [p1, p2].
    pub pressure_curve: [Point; 2],
    /// Bezier curve for pressure translation (opaque).
    pub bezier: *mut c_void,
}

impl InputDeviceToolNative {
    pub fn new() -> Self {
        InputDeviceToolNative {
            tool: core::ptr::null_mut(),
            button_map: BTreeMap::new(),
            pressure_curve: [Point::new(0.0, 0.0), Point::new(1.0, 1.0)],
            bezier: core::ptr::null_mut(),
        }
    }
}

impl Default for InputDeviceToolNative {
    fn default() -> Self {
        Self::new()
    }
}
