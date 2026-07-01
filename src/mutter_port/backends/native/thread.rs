use alloc::{boxed::Box, string::String, vec::Vec};

pub struct Thread {
    // TODO: port Thread from meta-thread.c
}

impl Thread {
    pub fn new() -> Self {
        Thread {}
    }
}

impl Default for Thread {
    fn default() -> Self {
        Self::new()
    }
}
