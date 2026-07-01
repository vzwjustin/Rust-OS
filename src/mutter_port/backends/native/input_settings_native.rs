use alloc::{boxed::Box, string::String, vec::Vec};

pub struct InputSettingsNative {
    // TODO: port InputSettingsNative from meta-input-settings-native.c
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
