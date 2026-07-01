use alloc::{boxed::Box, string::String, vec::Vec};

pub struct DrmBufferDumb {
    // TODO: port DrmBufferDumb from meta-drm-buffer-dumb.c
}

impl DrmBufferDumb {
    pub fn new() -> Self {
        DrmBufferDumb {}
    }
}

impl Default for DrmBufferDumb {
    fn default() -> Self {
        Self::new()
    }
}
