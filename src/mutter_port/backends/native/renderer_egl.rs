use alloc::{boxed::Box, string::String, vec::Vec};

pub struct RendererEgl {
    // TODO: port RendererEgl from meta-renderer-egl.c
}

impl RendererEgl {
    pub fn new() -> Self {
        RendererEgl {}
    }
}

impl Default for RendererEgl {
    fn default() -> Self {
        Self::new()
    }
}
