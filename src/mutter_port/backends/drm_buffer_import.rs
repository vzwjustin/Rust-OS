//! DRM Buffer Import ported from GNOME Mutter's src/backends/
//!
//! Provides DRM buffer creation from external DMA-buf file descriptors.
//! Allows importing buffers from other subsystems (e.g., Wayland, GPU drivers).
//! DRM I/O operations are left as TODO for backend implementers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-drm-buffer-import.h
//! Upstream header not found; minimal stub.

/// Placeholder for imported DRM buffer.
pub struct DrmBufferImport;

impl DrmBufferImport {
    /// Create a new imported DRM buffer.
    pub fn new() -> Self {
        Self
    }
}

impl Default for DrmBufferImport {
    fn default() -> Self {
        Self::new()
    }
}
