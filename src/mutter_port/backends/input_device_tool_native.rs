//! Input Device Tool Native — ported from GNOME Mutter
//!
//! Stylus and tablet tool support. Manages pressure curves, button mappings,
//! and tool-specific settings for pen and stylus devices.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-input-device-tool-native.h

pub const N_PRESSURECURVE_POINTS: usize = 256;

/// Native tablet tool representation.
pub struct InputDeviceToolNative {
    // libinput_tablet_tool pointer
    // serial: u64
    // type: ClutterInputDeviceToolType
    // pressure curve and button mapping state
}

impl InputDeviceToolNative {
    pub fn new() -> Self {
        InputDeviceToolNative {}
    }
}

impl Default for InputDeviceToolNative {
    fn default() -> Self {
        Self::new()
    }
}
