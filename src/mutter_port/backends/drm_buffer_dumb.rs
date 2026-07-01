//! DRM Buffer Dumb ported from GNOME Mutter's src/backends/
//!
//! Provides support for simple dumb framebuffer creation via DRM IOCTL.
//! DRM hardware operations are left as TODO stubs for backend implementers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-drm-buffer-dumb.h
//! Upstream header not found; minimal stub.

/// Placeholder for dumb DRM buffer.
pub struct DrmBufferDumb;

impl DrmBufferDumb {
    /// Create a new dumb DRM buffer.
    pub fn new() -> Self {
        Self
    }
}

impl Default for DrmBufferDumb {
    fn default() -> Self {
        Self::new()
    }
}
