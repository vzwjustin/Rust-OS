use alloc::{boxed::Box, string::String, vec::Vec};

pub struct FrameNative {
    // TODO: port FrameNative from meta-frame-native.c
}

impl FrameNative {
    pub fn new() -> Self {
        FrameNative {}
    }
}

impl Default for FrameNative {
    fn default() -> Self {
        Self::new()
    }
}
