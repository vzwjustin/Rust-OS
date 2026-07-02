//! Input Settings Native — ported from GNOME Mutter
//!
//! Native libinput-based implementation of input device settings.
//! Bridges between GSettings and libinput device configuration via libinput.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-input-settings-native.h

use crate::mutter_port::core::seat_impl::MetaSeatImpl;

/// Native input settings implementation using libinput.
/// Extends the base InputSettings with seat_impl for input thread coordination.
pub struct InputSettingsNative {
    /// Reference to the seat implementation for input thread task dispatch.
    pub seat_impl: *mut MetaSeatImpl,
}

impl InputSettingsNative {
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
