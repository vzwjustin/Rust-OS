//! Renderer Display Egl Private — EGL renderer display from GNOME Mutter
//!
//! Wraps an EGL display for use with Cogl rendering.
//! The actual EGL initialization and configuration is handled by EGL libraries.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-display-egl-private.h

use core::ffi::c_void;

/// Renderer Display Egl Private: EGL-based renderer display.
/// Extends CoglDisplayEGL with Mutter-specific configuration.
/// Note: EGL/Cogl rendering is left as TODO per no_std constraints.
#[derive(Debug, Clone)]
pub struct RendererDisplayEglPrivate {
    /// Parent CoglDisplayEGL (opaque pointer)
    pub cogl_display: *mut c_void,
}

impl RendererDisplayEglPrivate {
    pub fn new() -> Self {
        RendererDisplayEglPrivate {
            cogl_display: core::ptr::null_mut(),
        }
    }
}

impl Default for RendererDisplayEglPrivate {
    fn default() -> Self {
        Self::new()
    }
}
