//! Simple (non-atomic) KMS implementation for GNOME Mutter.
//!
//! Provides legacy KMS modeset support via separate ioctl calls (SET_CRTC, etc.)
//! for systems without atomic KMS. Falls back when atomic is unavailable.
//! Upstream header not found; minimal stub.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-kms-impl-device-simple.h

/// Simple (non-atomic) KMS implementation for a device.
pub struct MetaKmsImplDeviceSimple;

impl MetaKmsImplDeviceSimple {
    /// Create simple KMS implementation for a device.
    pub fn new() -> Self {
        MetaKmsImplDeviceSimple
    }
}

impl Default for MetaKmsImplDeviceSimple {
    fn default() -> Self {
        Self::new()
    }
}