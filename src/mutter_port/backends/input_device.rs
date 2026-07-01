//! Input Device — ported from GNOME Mutter
//!
//! Base input device abstraction for keyboard, mouse, touchpad, and other input
//! devices. This module provides the core type hierarchy for device management.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-device.c

use alloc::boxed::Box;

/// Base input device type. Carries reference to backend and optional Wacom device.
pub struct InputDevice {
    // TODO: backend reference
    // TODO: optional wacom_device pointer
}

impl InputDevice {
    pub fn new() -> Self {
        InputDevice {}
    }
}

impl Default for InputDevice {
    fn default() -> Self {
        Self::new()
    }
}
