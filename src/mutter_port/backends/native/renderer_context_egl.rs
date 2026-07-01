use alloc::{boxed::Box, string::String, vec::Vec};

pub struct RendererContextEgl {
    // TODO: port RendererContextEgl from meta-renderer-context-egl.c
}

impl RendererContextEgl {
    pub fn new() -> Self {
        RendererContextEgl {}
    }
}

impl Default for RendererContextEgl {
    fn default() -> Self {
        Self::new()
    }
}
