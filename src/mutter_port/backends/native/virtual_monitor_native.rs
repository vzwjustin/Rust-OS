use alloc::{boxed::Box, string::String, vec::Vec};

pub struct VirtualMonitorNative {
    // TODO: port VirtualMonitorNative from meta-virtual-monitor-native.c
}

impl VirtualMonitorNative {
    pub fn new() -> Self {
        VirtualMonitorNative {}
    }
}

impl Default for VirtualMonitorNative {
    fn default() -> Self {
        Self::new()
    }
}
