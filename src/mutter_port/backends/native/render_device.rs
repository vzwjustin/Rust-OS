use alloc::{boxed::Box, string::String, vec::Vec};

pub struct RenderDevice {
    // TODO: port RenderDevice from meta-render-device.c
}

impl RenderDevice {
    pub fn new() -> Self {
        RenderDevice {}
    }
}

impl Default for RenderDevice {
    fn default() -> Self {
        Self::new()
    }
}
