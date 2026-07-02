//! EGL renderer for Mutter native backend.
//!
//! Implements CoglRendererEGL subclass for GPU rendering via EGL,
//! managing rendering context and GPU device state.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-egl.c

use alloc::{boxed::Box, string::String, vec::Vec};
use core::ffi::c_void;

/// EGL renderer implementation extending CoglRendererEGL.
pub struct RendererEgl {
    /// Parent CoglRendererEGL (opaque C object)
    pub parent_instance: *mut c_void,
    /// Renderer GPU data (device, context, modifiers)
    pub renderer_gpu_data: *mut c_void,
}

impl RendererEgl {
    pub fn new() -> Self {
        RendererEgl {
            parent_instance: core::ptr::null_mut(),
            renderer_gpu_data: core::ptr::null_mut(),
        }
    }
}

impl Default for RendererEgl {
    fn default() -> Self {
        Self::new()
    }
}
