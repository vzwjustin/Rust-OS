use alloc::{boxed::Box, string::String, vec::Vec};

pub struct PointerConstraintNative {
    // TODO: port PointerConstraintNative from meta-pointer-constraint-native.c
}

impl PointerConstraintNative {
    pub fn new() -> Self {
        PointerConstraintNative {}
    }
}

impl Default for PointerConstraintNative {
    fn default() -> Self {
        Self::new()
    }
}
