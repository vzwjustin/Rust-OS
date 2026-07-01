use alloc::{boxed::Box, string::String, vec::Vec};

pub struct DrmBufferImport {
    // TODO: port DrmBufferImport from meta-drm-buffer-import.c
}

impl DrmBufferImport {
    pub fn new() -> Self {
        DrmBufferImport {}
    }
}

impl Default for DrmBufferImport {
    fn default() -> Self {
        Self::new()
    }
}
