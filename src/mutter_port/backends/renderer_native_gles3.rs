//! Renderer Native Gles3 — Native renderer GLES3 blitting from GNOME Mutter
//!
//! Provides optimized GBM buffer blitting using GLES3 and EGL.
//! Used by the native X11/Wayland renderer for efficient framebuffer updates.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-native-gles3.h

use alloc::vec::Vec;

/// Buffer type support information for a specific DRM format/modifier pair.
/// Tracks whether a buffer combination can be blitted using GLES3.
#[derive(Debug, Clone, Copy)]
pub struct BufferTypeSupport {
    /// DRM format code (e.g., DRM_FORMAT_XRGB8888)
    pub drm_format: u32,
    /// DRM format modifier (e.g., DRM_FORMAT_MOD_LINEAR)
    pub drm_modifier: u64,
    /// Whether this buffer type can be blitted
    pub can_blit: bool,
}

/// Context-specific data for renderer native GLES3 operations.
/// Tracks EGLContext-to-buffer-support mappings for efficient blitting.
#[derive(Debug, Clone)]
pub struct ContextData {
    /// Array of buffer types and their support status for this EGL context
    pub buffer_support: Vec<BufferTypeSupport>,
    /// GLES3 shader program object for blitting operations
    pub shader_program: u32,
}

impl ContextData {
    pub fn new() -> Self {
        ContextData {
            buffer_support: Vec::new(),
            shader_program: 0,
        }
    }
}

impl Default for ContextData {
    fn default() -> Self {
        Self::new()
    }
}