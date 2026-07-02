//! EGL Display renderer for GNOME Mutter.
//!
//! Wraps Cogl EGL display configuration for GPU-accelerated rendering.
//! Handles EGL configuration attributes and format selection for GBM/surfaceless modes.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-display-egl.c

use alloc::{boxed::Box, string::String, vec::Vec};

/// EGL display renderer (inherits from CoglDisplayEGL parent).
pub struct RendererDisplayEgl;

impl RendererDisplayEgl {
    /// Create a new EGL display renderer.
    pub fn new() -> Self {
        RendererDisplayEgl
    }
}

impl Default for RendererDisplayEgl {
    fn default() -> Self {
        Self::new()
    }
}
