//! Backend Native Private ported from GNOME Mutter's src/backends/
//!
//! Private native backend implementation details for Linux/DRM hardware abstraction.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-backend-native-private.h
//! Upstream header not found; minimal stub.

/// Opaque native backend private state.
pub struct MetaBackendNativePrivate;

impl MetaBackendNativePrivate {
    /// Create a new native backend private state.
    pub fn new() -> Self {
        MetaBackendNativePrivate
    }
}

impl Default for MetaBackendNativePrivate {
    fn default() -> Self {
        Self::new()
    }
}
