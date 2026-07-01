//! Input Device Native — ported from GNOME Mutter
//!
//! Native input device implementation using libinput. Manages device-specific state
//! including coordinate mapping, button states, and scroll accumulators.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-input-device-native.h

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaInputDeviceMapping {
    META_INPUT_DEVICE_MAPPING_ABSOLUTE = 0,
    META_INPUT_DEVICE_MAPPING_RELATIVE = 1,
}

/// Native input device state, wrapping libinput device.
pub struct InputDeviceNative {
    // libinput_device pointer would go here
    // seat_impl reference
    // last_tool: ClutterInputDeviceTool
    // pad_features: GArray
    // modes: GArray
    // group: intptr_t
    // device_matrix: graphene_matrix_t
    // width: i32
    // height: i32
    // device_aspect_ratio: f64
    // output_ratio: f64
    // mapping_mode: MetaInputDeviceMapping
    // button_state: ClutterModifierType
    // value120 scroll accumulator fields
}

impl InputDeviceNative {
    pub fn new() -> Self {
        InputDeviceNative {}
    }
}

impl Default for InputDeviceNative {
    fn default() -> Self {
        Self::new()
    }
}
