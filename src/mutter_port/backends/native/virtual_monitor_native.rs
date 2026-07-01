//! Virtual monitor implementation for headless/remote rendering.
//!
//! Manages virtual (non-physical) displays for headless rendering or
//! remote access, including dynamic mode management and mode ID tracking.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-virtual-monitor-native.c

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// Virtual monitor instance for headless/remote output.
pub struct VirtualMonitorNative {
    /// Parent MetaVirtualMonitor (opaque C object)
    pub parent: *mut c_void,
    /// Unique ID for this virtual monitor
    pub id: u64,
}

impl VirtualMonitorNative {
    pub fn new() -> Self {
        VirtualMonitorNative {
            parent: core::ptr::null_mut(),
            id: 0,
        }
    }
}

impl Default for VirtualMonitorNative {
    fn default() -> Self {
        Self::new()
    }
}
