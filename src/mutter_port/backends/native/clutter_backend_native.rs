use alloc::{boxed::Box, string::String, vec::Vec};

pub struct ClutterBackendNative {
    // TODO: port ClutterBackendNative from meta-clutter-backend-native.c
}

impl ClutterBackendNative {
    pub fn new() -> Self {
        ClutterBackendNative {}
    }
}

impl Default for ClutterBackendNative {
    fn default() -> Self {
        Self::new()
    }
}
