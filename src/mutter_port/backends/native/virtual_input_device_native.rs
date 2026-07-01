use alloc::{boxed::Box, string::String, vec::Vec};

pub struct VirtualInputDeviceNative {
    // TODO: port VirtualInputDeviceNative from meta-virtual-input-device-native.c
}

impl VirtualInputDeviceNative {
    pub fn new() -> Self {
        VirtualInputDeviceNative {}
    }
}

impl Default for VirtualInputDeviceNative {
    fn default() -> Self {
        Self::new()
    }
}
