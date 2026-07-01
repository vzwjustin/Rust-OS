//! DRM Buffer Private (internal types) ported from GNOME Mutter's src/backends/
//!
//! Contains private type definitions and helpers for DRM buffer management.
//! This module is for internal use within the DRM subsystem; hardware-specific
//! operations are deferred to backend implementations.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-drm-buffer-private.h
//! Upstream header not found; minimal stub.

/// Placeholder for private DRM buffer types.
pub struct DrmBufferPrivate;

impl DrmBufferPrivate {
    /// Create a new private DRM buffer wrapper.
    pub fn new() -> Self {
        Self
    }
}

impl Default for DrmBufferPrivate {
    fn default() -> Self {
        Self::new()
    }
}
