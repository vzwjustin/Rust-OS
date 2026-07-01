//! DRM Buffer GBM ported from GNOME Mutter's src/backends/
//!
//! Provides GBM (Graphics Buffer Management) backed DRM buffers with
//! support for hardware-accelerated scanout. DRM/GBM I/O operations
//! are deferred to backend implementations.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-drm-buffer-gbm.h
//! Upstream header not found; minimal stub.

/// Placeholder for GBM-backed DRM buffer.
pub struct DrmBufferGbm;

impl DrmBufferGbm {
    /// Create a new GBM-backed DRM buffer.
    pub fn new() -> Self {
        Self
    }
}

impl Default for DrmBufferGbm {
    fn default() -> Self {
        Self::new()
    }
}
