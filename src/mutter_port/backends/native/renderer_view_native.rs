use alloc::{boxed::Box, string::String, vec::Vec};

pub struct RendererViewNative {
    // TODO: port RendererViewNative from meta-renderer-view-native.c
}

impl RendererViewNative {
    pub fn new() -> Self {
        RendererViewNative {}
    }
}

impl Default for RendererViewNative {
    fn default() -> Self {
        Self::new()
    }
}
