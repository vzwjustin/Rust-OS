//! Private KMS subsystem internals.
//!
//! Internal types and state management for the KMS subsystem,
//! including device management, resource tracking, and
//! update coordination.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-private.h
//! Note: Upstream header not found; minimal stub.

/// Private KMS state
pub struct MetaKmsPrivate {
    // TODO: Devices, resources, pending updates
}

impl MetaKmsPrivate {
    /// Create private KMS state
    pub fn new() -> Self {
        MetaKmsPrivate {}
    }
}

impl Default for MetaKmsPrivate {
    fn default() -> Self {
        Self::new()
    }
}