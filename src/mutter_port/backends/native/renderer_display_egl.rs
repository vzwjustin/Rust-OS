use alloc::{boxed::Box, string::String, vec::Vec};

pub struct RendererDisplayEgl {
    // TODO: port RendererDisplayEgl from meta-renderer-display-egl.c
}

impl RendererDisplayEgl {
    pub fn new() -> Self {
        RendererDisplayEgl {}
    }
}

impl Default for RendererDisplayEgl {
    fn default() -> Self {
        Self::new()
    }
}
