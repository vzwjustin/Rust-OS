//! EGL/GBM integration for buffer management.
//!
//! Provides utilities for managing GBM buffer objects with EGL image bindings,
//! including DMA-BUF import and format negotiation for GPU memory.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-egl-gbm.c

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// GBM BO user data attached to buffer objects.
/// Contains EGL image and display references for destruction.
pub struct GbmBoUserData {
    /// EGL image handle (KHR extension)
    pub egl_image: *mut c_void,
    /// Reference to MetaEgl instance
    pub egl: *mut c_void,
    /// EGL display connection
    pub egl_display: *mut c_void,
}

impl GbmBoUserData {
    pub fn new() -> Self {
        GbmBoUserData {
            egl_image: core::ptr::null_mut(),
            egl: core::ptr::null_mut(),
            egl_display: core::ptr::null_mut(),
        }
    }
}

impl Default for GbmBoUserData {
    fn default() -> Self {
        Self::new()
    }
}

/// EGL/GBM buffer helper (placeholder for future buffer management).
pub struct EglGbm {
    // Opaque handle for GBM device context
    pub gbm_device: *mut c_void,
}

impl EglGbm {
    pub fn new() -> Self {
        EglGbm {
            gbm_device: core::ptr::null_mut(),
        }
    }
}

impl Default for EglGbm {
    fn default() -> Self {
        Self::new()
    }
}
