//! DRM Buffer GBM — hardware-accelerated framebuffer backed by GBM (Generic Buffer Management).
//!
//! Represents a GPU-allocated buffer that can be scanned out directly to a display.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-drm-buffer-gbm.c

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

pub struct DrmBufferGbm {
    /// GBM surface (opaque pointer to gbm_surface)
    pub surface: *mut c_void,
    /// GBM buffer object (opaque pointer to gbm_bo)
    pub bo: *mut c_void,
}

impl DrmBufferGbm {
    pub fn new() -> Self {
        DrmBufferGbm {
            surface: core::ptr::null_mut(),
            bo: core::ptr::null_mut(),
        }
    }
}

impl Default for DrmBufferGbm {
    fn default() -> Self {
        Self::new()
    }
}
