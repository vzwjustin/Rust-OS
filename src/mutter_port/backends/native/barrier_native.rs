use alloc::{boxed::Box, string::String, vec::Vec};

pub struct BarrierManagerNative {
    backend: *const u8,
    // TODO: port barrier implementation from meta-barrier-native.c
}

impl BarrierManagerNative {
    pub fn new(backend: *const u8) -> Self {
        BarrierManagerNative { backend }
    }

    pub fn destroy(&mut self) {
        // TODO: port barrier cleanup from meta-barrier-native.c
    }
}
