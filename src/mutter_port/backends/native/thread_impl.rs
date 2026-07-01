use alloc::{boxed::Box, string::String, vec::Vec};

pub struct ThreadImpl {
    // TODO: port ThreadImpl from meta-thread-impl.c
}

impl ThreadImpl {
    pub fn new() -> Self {
        ThreadImpl {}
    }
}

impl Default for ThreadImpl {
    fn default() -> Self {
        Self::new()
    }
}
