//! Renderer Native Private — ported from GNOME Mutter
//!
//! GPU data structures and copy modes for dual-GPU rendering scenarios.
//! Handles framebuffer sharing between primary and secondary GPUs.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-native-private.h

use alloc::string::String;

/// How a secondary GPU's shared framebuffer copy is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaSharedFramebufferCopyMode {
    /// Zero-copy: primary GPU exports, secondary GPU imports as KMS FB.
    ZERO = 0,
    /// Secondary GPU makes the copy.
    SECONDARY_GPU = 1,
    /// Copy made in primary GPU context (CPU or GPU blit).
    PRIMARY = 2,
}

/// Placeholder types for opaque C structures.
#[derive(Debug)]
pub struct MetaRendererNative;

#[derive(Debug)]
pub struct MetaRenderDevice;

#[derive(Debug)]
pub struct MetaGpuKms;

/// Renderer native mode (opaque value).
pub type MetaRendererNativeMode = u32;

/// Per-GPU data for the native renderer.
/// Tracks rendering device, copy mode, and secondary GPU specifics.
#[derive(Debug, Clone)]
pub struct MetaRendererNativeGpuData {
    /// Reference to the renderer (opaque).
    pub renderer_native: u32,
    /// Rendering device (opaque).
    pub render_device: u32,
    /// GPU KMS handle (opaque).
    pub gpu_kms: u32,
    /// Renderer mode.
    pub mode: MetaRendererNativeMode,
    /// Copy mode for framebuffer sharing.
    pub copy_mode: MetaSharedFramebufferCopyMode,
    /// Force CPU path for primary GPU copy.
    pub copy_mode_primary_force_cpu: bool,
    /// Has EGL_EXT_image_dma_buf_import_modifiers support.
    pub has_egl_ext_image_dma_buf_import_modifiers: bool,
    /// Is NVIDIA GPU.
    pub is_nvidia: bool,
    /// EGL context handle (opaque).
    pub egl_context: u32,
    /// CRTC flush handler ID.
    pub crtc_needs_flush_handler_id: usize,
}

impl MetaRendererNativeGpuData {
    /// Create new GPU data.
    pub fn new(gpu_kms: u32, mode: MetaRendererNativeMode) -> Self {
        MetaRendererNativeGpuData {
            renderer_native: 0,
            render_device: 0,
            gpu_kms,
            mode,
            copy_mode: MetaSharedFramebufferCopyMode::ZERO,
            copy_mode_primary_force_cpu: false,
            has_egl_ext_image_dma_buf_import_modifiers: false,
            is_nvidia: false,
            egl_context: 0,
            crtc_needs_flush_handler_id: 0,
        }
    }
}

impl Default for MetaRendererNativeGpuData {
    fn default() -> Self {
        Self::new(0, 0)
    }
}
