use alloc::{boxed::Box, string::String, vec::Vec};

pub struct RendererNativeGles3 {
    // TODO: port RendererNativeGles3 from meta-renderer-native-gles3.c
}

impl RendererNativeGles3 {
    pub fn new() -> Self {
        RendererNativeGles3 {}
    }
}

impl Default for RendererNativeGles3 {
    fn default() -> Self {
        Self::new()
    }
}
