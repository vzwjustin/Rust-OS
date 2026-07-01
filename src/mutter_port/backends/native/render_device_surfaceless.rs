//! Surfaceless EGL rendering device for headless GPU rendering.
//!
//! Provides GPU rendering without framebuffer surfaces, useful for
//! offscreen rendering, compute, or when KMS is unavailable.
//! Creates EGLDisplay via EGL_KHR_platform_gbm extension.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-render-device-surfaceless.c

use alloc::{boxed::Box, string::String, vec::Vec};

/// Surfaceless EGL render device (no KMS framebuffer)
pub struct RenderDeviceSurfaceless {
    /// Parent render device (opaque)
    pub parent: *mut core::ffi::c_void,
    /// EGL display handle (opaque)
    pub egl_display: *mut core::ffi::c_void,
    /// EGL context (opaque)
    pub egl_context: *mut core::ffi::c_void,
}

impl RenderDeviceSurfaceless {
    pub fn new() -> Self {
        RenderDeviceSurfaceless {
            parent: core::ptr::null_mut(),
            egl_display: core::ptr::null_mut(),
            egl_context: core::ptr::null_mut(),
        }
    }
}

impl Default for RenderDeviceSurfaceless {
    fn default() -> Self {
        Self::new()
    }
}
