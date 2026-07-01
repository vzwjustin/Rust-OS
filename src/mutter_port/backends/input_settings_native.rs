//! Input Settings Native — ported from GNOME Mutter
//!
//! Native libinput-based implementation of input device settings.
//! Bridges between GSettings and libinput device configuration.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-input-settings-native.h

use super::input_settings::InputSettings;

/// Native input settings implementation using libinput.
pub struct InputSettingsNative {
    // base InputSettings fields
    // libinput-specific state
}

impl InputSettingsNative {
    pub fn new() -> Self {
        InputSettingsNative {}
    }
}

impl Default for InputSettingsNative {
    fn default() -> Self {
        Self::new()
    }
}
