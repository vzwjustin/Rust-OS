use alloc::{boxed::Box, string::String, vec::Vec};

pub struct RenderDeviceSurfaceless {
    // TODO: port RenderDeviceSurfaceless from meta-render-device-surfaceless.c
}

impl RenderDeviceSurfaceless {
    pub fn new() -> Self {
        RenderDeviceSurfaceless {}
    }
}

impl Default for RenderDeviceSurfaceless {
    fn default() -> Self {
        Self::new()
    }
}
