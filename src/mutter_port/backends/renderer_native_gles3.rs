//! Renderer Native Gles3 — Native renderer GLES3 blitting from GNOME Mutter
//!
//! Provides optimized GBM buffer blitting using GLES3 and EGL.
//! Used by the native X11/Wayland renderer for efficient framebuffer updates.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-native-gles3.h

/// Context-specific data for renderer native GLES3 operations.
/// Tracks EGLContext-to-buffer-support mappings.
pub struct ContextData {
    // TODO: port fields from meta-renderer-native-gles3.c
}

impl ContextData {
    pub fn new() -> Self {
        ContextData {}
    }
}

impl Default for ContextData {
    fn default() -> Self {
        Self::new()
    }
}