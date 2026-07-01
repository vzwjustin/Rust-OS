use alloc::{boxed::Box, string::String, vec::Vec};

pub struct RenderDeviceGbm {
    // TODO: port RenderDeviceGbm from meta-render-device-gbm.c
}

impl RenderDeviceGbm {
    pub fn new() -> Self {
        RenderDeviceGbm {}
    }
}

impl Default for RenderDeviceGbm {
    fn default() -> Self {
        Self::new()
    }
}
