use alloc::{boxed::Box, string::String, vec::Vec};

pub struct DrmBufferGbm {
    // TODO: port DrmBufferGbm from meta-drm-buffer-gbm.c
}

impl DrmBufferGbm {
    pub fn new() -> Self {
        DrmBufferGbm {}
    }
}

impl Default for DrmBufferGbm {
    fn default() -> Self {
        Self::new()
    }
}
