use alloc::{boxed::Box, string::String, vec::Vec};

pub struct InputDeviceToolNative {
    // TODO: port InputDeviceToolNative from meta-input-device-tool-native.c
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
