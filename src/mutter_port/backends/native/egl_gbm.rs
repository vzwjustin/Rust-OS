use alloc::{boxed::Box, string::String, vec::Vec};

pub struct EglGbm {
    // TODO: port EglGbm from meta-egl-gbm.c
}

impl EglGbm {
    pub fn new() -> Self {
        EglGbm {}
    }
}

impl Default for EglGbm {
    fn default() -> Self {
        Self::new()
    }
}
