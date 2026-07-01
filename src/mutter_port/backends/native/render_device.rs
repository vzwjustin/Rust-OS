//! Render device abstraction for GPU/display rendering.
//!
//! Manages EGL display connections, hardware detection, and DMA-BUF
//! allocation for rendering on specific GPU devices.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-render-device.c

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// Render device instance providing GPU rendering context.
pub struct RenderDevice {
    /// Parent GObject (opaque)
    pub parent: *mut c_void,
    /// Associated MetaBackend
    pub backend: *mut c_void,
    /// Device file descriptor wrapper
    pub device_file: *mut c_void,
    /// EGL display connection for this device
    pub egl_display: *mut c_void,
    /// Whether hardware rendering is supported
    pub is_hardware_accelerated: bool,
}

impl RenderDevice {
    pub fn new() -> Self {
        RenderDevice {
            parent: core::ptr::null_mut(),
            backend: core::ptr::null_mut(),
            device_file: core::ptr::null_mut(),
            egl_display: core::ptr::null_mut(),
            is_hardware_accelerated: false,
        }
    }
}

impl Default for RenderDevice {
    fn default() -> Self {
        Self::new()
    }
}
