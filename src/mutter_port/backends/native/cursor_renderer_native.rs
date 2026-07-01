use alloc::{boxed::Box, string::String, vec::Vec};

pub struct CursorRendererNative {
    // TODO: port CursorRendererNative from meta-cursor-renderer-native.c
}

impl CursorRendererNative {
    pub fn new() -> Self {
        CursorRendererNative {}
    }
}

impl Default for CursorRendererNative {
    fn default() -> Self {
        Self::new()
    }
}
