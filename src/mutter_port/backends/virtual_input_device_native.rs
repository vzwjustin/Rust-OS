//! Virtual Input Device Native — ported from GNOME Mutter
//!
//! Native implementation of virtual input devices for synthetic input generation.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-virtual-input-device-native.h

/// Native virtual input device.
pub struct MetaVirtualInputDeviceNative;

impl MetaVirtualInputDeviceNative {
    pub fn new() -> Self {
        MetaVirtualInputDeviceNative
    }
}

impl Default for MetaVirtualInputDeviceNative {
    fn default() -> Self {
        Self::new()
    }
}