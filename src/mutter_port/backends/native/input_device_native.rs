use alloc::{boxed::Box, string::String, vec::Vec};

pub struct InputDeviceNative {
    // TODO: port InputDeviceNative from meta-input-device-native.c
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
