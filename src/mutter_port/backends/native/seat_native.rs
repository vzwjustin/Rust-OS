use alloc::{boxed::Box, string::String, vec::Vec};

pub struct SeatNative {
    // TODO: port SeatNative from meta-seat-native.c
}

impl SeatNative {
    pub fn new() -> Self {
        SeatNative {}
    }
}

impl Default for SeatNative {
    fn default() -> Self {
        Self::new()
    }
}
