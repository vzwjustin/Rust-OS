use alloc::{boxed::Box, string::String, vec::Vec};

pub struct XkbUtils {
    // TODO: port XkbUtils from meta-xkb-utils.c
}

impl XkbUtils {
    pub fn new() -> Self {
        XkbUtils {}
    }
}

impl Default for XkbUtils {
    fn default() -> Self {
        Self::new()
    }
}
