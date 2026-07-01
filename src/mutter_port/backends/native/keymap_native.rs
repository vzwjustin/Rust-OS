use alloc::{boxed::Box, string::String, vec::Vec};

pub struct KeymapNative {
    // TODO: port KeymapNative from meta-keymap-native.c
}

impl KeymapNative {
    pub fn new() -> Self {
        KeymapNative {}
    }
}

impl Default for KeymapNative {
    fn default() -> Self {
        Self::new()
    }
}
