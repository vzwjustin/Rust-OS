//! No-op KMS implementation for headless or unsupported devices.
//!
//! Provides a dummy backend that accepts all KMS operations
//! but performs no actual hardware programming. Used when
//! KMS is unavailable or disabled.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-impl-device-dummy.h
//! Note: Upstream header not found; minimal stub.

/// Dummy (no-op) KMS implementation
pub struct MetaKmsImplDeviceDummy {
    // No state needed for dummy implementation
}

impl MetaKmsImplDeviceDummy {
    /// Create dummy KMS implementation
    pub fn new() -> Self {
        MetaKmsImplDeviceDummy {}
    }
}

impl Default for MetaKmsImplDeviceDummy {
    fn default() -> Self {
        Self::new()
    }
}
