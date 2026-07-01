use alloc::{boxed::Box, string::String, vec::Vec};

pub struct SeatImpl {
    // TODO: port SeatImpl from meta-seat-impl.c
}

impl SeatImpl {
    pub fn new() -> Self {
        SeatImpl {}
    }
}

impl Default for SeatImpl {
    fn default() -> Self {
        Self::new()
    }
}
