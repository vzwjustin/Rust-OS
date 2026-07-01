//! Drm Buffer
//!
//! Native Rust implementation (no direct Mutter C counterpart).
//! Represents a DRM-allocated framebuffer for GPU rendering and scanout.

use alloc::string::String;

/// Drm Buffer — holds DRM handle and geometry for framebuffer allocation.
#[derive(Debug, Clone)]
pub struct DrmBuffer {
    pub handle: u32,
    pub fd: i32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

impl DrmBuffer {
    pub fn new() -> Self {
        DrmBuffer {
            handle: 0,
            fd: -1,
            width: 0,
            height: 0,
            stride: 0,
        }
    }
}

impl Default for DrmBuffer {
    fn default() -> Self {
        Self::new()
    }
}
