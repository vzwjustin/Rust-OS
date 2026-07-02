//! Input Device Native — ported from GNOME Mutter
//!
//! Native input device implementation using libinput. Manages device-specific state
//! including coordinate mapping, button states, and scroll accumulators for precise input handling.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-input-device-native.h

use alloc::vec::Vec;
use core::ffi::c_void;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaInputDeviceMapping {
    META_INPUT_DEVICE_MAPPING_ABSOLUTE = 0,
    META_INPUT_DEVICE_MAPPING_RELATIVE = 1,
}

/// 4x4 transformation matrix for device-to-output coordinate mapping.
#[derive(Debug, Clone, Copy)]
pub struct Matrix4x4 {
    pub m: [[f32; 4]; 4],
}

impl Default for Matrix4x4 {
    fn default() -> Self {
        Matrix4x4 {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
}

/// Scroll accumulator for high-resolution wheel events (value120 protocol).
#[derive(Debug, Clone, Copy)]
pub struct ScrollAccumulator {
    pub acc_dx: i32,
    pub acc_dy: i32,
    pub last_dx: i32,
    pub last_dy: i32,
}

impl Default for ScrollAccumulator {
    fn default() -> Self {
        ScrollAccumulator {
            acc_dx: 0,
            acc_dy: 0,
            last_dx: 0,
            last_dy: 0,
        }
    }
}

/// Native input device state, wrapping libinput device.
pub struct InputDeviceNative {
    /// Pointer to libinput_device (opaque).
    pub libinput_device: *mut c_void,
    /// Pointer to MetaSeatImpl (opaque).
    pub seat_impl: *mut c_void,
    /// Pointer to last input tool (opaque).
    pub last_tool: *mut c_void,
    /// Pad features (VecVec of feature definitions).
    pub pad_features: Vec<u32>,
    /// Pad modes (Vec of mode indices).
    pub modes: Vec<u32>,
    /// Device group identifier (opaque intptr).
    pub group: usize,
    /// 4x4 transformation matrix for coordinate mapping.
    pub device_matrix: Matrix4x4,
    /// Device logical width in pixels.
    pub width: i32,
    /// Device logical height in pixels.
    pub height: i32,
    /// Device aspect ratio (width:height).
    pub device_aspect_ratio: f64,
    /// Output aspect ratio (width:height).
    pub output_ratio: f64,
    /// Current input mapping mode (absolute or relative).
    pub mapping_mode: MetaInputDeviceMapping,
    /// Current button state bitmask (ClutterModifierType).
    pub button_state: u32,
    /// High-resolution scroll event accumulator.
    pub value120: ScrollAccumulator,
}

impl InputDeviceNative {
    pub fn new() -> Self {
        InputDeviceNative {
            libinput_device: core::ptr::null_mut(),
            seat_impl: core::ptr::null_mut(),
            last_tool: core::ptr::null_mut(),
            pad_features: Vec::new(),
            modes: Vec::new(),
            group: 0,
            device_matrix: Matrix4x4::default(),
            width: 0,
            height: 0,
            device_aspect_ratio: 1.0,
            output_ratio: 1.0,
            mapping_mode: MetaInputDeviceMapping::META_INPUT_DEVICE_MAPPING_RELATIVE,
            button_state: 0,
            value120: ScrollAccumulator::default(),
        }
    }
}

impl Default for InputDeviceNative {
    fn default() -> Self {
        Self::new()
    }
}
