//! Device Pool
//!
//! Native Rust implementation (no direct Mutter C counterpart).
//! Manages a pool of input devices for coordinated access and lifecycle tracking.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;

/// Device Pool — manages input device lifecycle and access coordination.
#[derive(Debug, Clone)]
pub struct DevicePool {
    pub devices: BTreeMap<usize, *mut core::ffi::c_void>,
    pub active_count: usize,
}

impl DevicePool {
    pub fn new() -> Self {
        DevicePool {
            devices: BTreeMap::new(),
            active_count: 0,
        }
    }
}

impl Default for DevicePool {
    fn default() -> Self {
        Self::new()
    }
}
