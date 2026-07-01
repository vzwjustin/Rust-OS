use alloc::{boxed::Box, string::String, vec::Vec};

pub struct SpriteNative {
    // TODO: port SpriteNative from meta-sprite-native.c
}

impl SpriteNative {
    pub fn new() -> Self {
        SpriteNative {}
    }
}

impl Default for SpriteNative {
    fn default() -> Self {
        Self::new()
    }
}
