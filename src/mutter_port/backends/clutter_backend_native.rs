//! Clutter Backend Native ported from GNOME Mutter's src/backends/
//!
//! Native Clutter graphics backend integration for DRM/KMS rendering and event handling.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-clutter-backend-native.h
//! Upstream header not found; minimal stub.

/// Opaque Clutter backend native implementation.
pub struct ClutterBackendNative;

impl ClutterBackendNative {
    /// Create a new Clutter backend native.
    pub fn new() -> Self {
        ClutterBackendNative
    }
}

impl Default for ClutterBackendNative {
    fn default() -> Self {
        Self::new()
    }
}
