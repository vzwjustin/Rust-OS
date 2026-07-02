//! Native input device management for keyboard, mouse, touchpad, and other devices.
//!
//! Wraps libinput_device and manages device-specific state including coordinate
//! transformation, LED updates, and device mapping mode (absolute vs relative).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-input-device-native.h

use alloc::{boxed::Box, string::String, vec::Vec};

/// Device mapping mode for relative/absolute coordinate input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum InputDeviceMapping {
    /// Absolute positioning (tablets, touch)
    ABSOLUTE = 0,
    /// Relative positioning (mice, trackballs)
    RELATIVE = 1,
}

/// Native input device state wrapper
pub struct InputDeviceNative {
    /// libinput device handle (opaque)
    pub libinput_device: *mut core::ffi::c_void,
    /// Seat implementation (opaque)
    pub seat_impl: *mut core::ffi::c_void,
    /// Last input tool used (opaque)
    pub last_tool: *mut core::ffi::c_void,
    /// Pad features array (opaque)
    pub pad_features: *mut core::ffi::c_void,
    /// Device modes array (opaque)
    pub modes: *mut core::ffi::c_void,
    /// Device group identifier
    pub group: isize,
    /// Device transformation matrix (16 f32 values)
    pub device_matrix: [f32; 16],
    /// Logical width in display space
    pub width: i32,
    /// Logical height in display space
    pub height: i32,
    /// Device aspect ratio (width:height)
    pub device_aspect_ratio: f64,
    /// Output aspect ratio (width:height)
    pub output_ratio: f64,
    /// Current mapping mode
    pub mapping_mode: InputDeviceMapping,
    /// Button state modifier mask
    pub button_state: u32,
    /// Accumulated horizontal scroll delta (sub-pixel)
    pub value120_acc_dx: i32,
    /// Accumulated vertical scroll delta (sub-pixel)
    pub value120_acc_dy: i32,
    /// Last horizontal scroll delta
    pub value120_last_dx: i32,
    /// Last vertical scroll delta
    pub value120_last_dy: i32,
}

impl InputDeviceNative {
    pub fn new() -> Self {
        InputDeviceNative {
            libinput_device: core::ptr::null_mut(),
            seat_impl: core::ptr::null_mut(),
            last_tool: core::ptr::null_mut(),
            pad_features: core::ptr::null_mut(),
            modes: core::ptr::null_mut(),
            group: 0,
            device_matrix: [0.0; 16],
            width: 0,
            height: 0,
            device_aspect_ratio: 0.0,
            output_ratio: 0.0,
            mapping_mode: InputDeviceMapping::RELATIVE,
            button_state: 0,
            value120_acc_dx: 0,
            value120_acc_dy: 0,
            value120_last_dx: 0,
            value120_last_dy: 0,
        }
    }
}

impl Default for InputDeviceNative {
    fn default() -> Self {
        Self::new()
    }
}
