//! Private KMS display mode representation.
//!
//! Internal structures and utilities for managing display modes
//! (resolution, refresh rate, timings) within the KMS subsystem.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-mode-private.h
//! Note: Upstream header not found; minimal stub.

/// Private display mode data for KMS
pub struct MetaKmsModePrivate {
    // TODO: Mode timings, refresh rate, format info
}

impl MetaKmsModePrivate {
    /// Create private mode data
    pub fn new() -> Self {
        MetaKmsModePrivate {}
    }
}

impl Default for MetaKmsModePrivate {
    fn default() -> Self {
        Self::new()
    }
}