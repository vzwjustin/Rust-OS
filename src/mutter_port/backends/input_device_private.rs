//! Input Device Private — ported from GNOME Mutter
//!
//! Private definitions and virtual methods for input devices. Defines the base
//! class structure that backends extend. Provides vfunc pointers for device-specific
//! behavior like capability reporting and property queries.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-device-private.h

use core::ffi::c_void;

/// Virtual method table for input devices (extending ClutterInputDeviceClass).
/// GObject vtable with device-specific vfuncs.
pub struct InputDeviceClass {
    /// Parent class (opaque ClutterInputDeviceClass).
    pub parent_class: *mut c_void,
}

impl InputDeviceClass {
    /// Create a new input device class structure.
    pub fn new() -> Self {
        InputDeviceClass {
            parent_class: core::ptr::null_mut(),
        }
    }
}

impl Default for InputDeviceClass {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to get backend from device.
/// Returns opaque pointer to MetaBackend.
pub fn meta_input_device_get_backend(_device: &c_void) -> *mut c_void {
    // TODO: Extract backend from device GObject properties
    core::ptr::null_mut()
}
