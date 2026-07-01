use alloc::{boxed::Box, string::String, vec::Vec};

pub struct StageNative {
    // TODO: port StageNative from meta-stage-native.c
}

impl StageNative {
    pub fn new() -> Self {
        StageNative {}
    }
}

impl Default for StageNative {
    fn default() -> Self {
        Self::new()
    }
}
