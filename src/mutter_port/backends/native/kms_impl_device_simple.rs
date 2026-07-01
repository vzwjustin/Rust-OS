//! Non-atomic (legacy) KMS implementation for a single device.
//!
//! Handles KMS updates using separate ioctl calls (SET_CRTC, etc.)
//! for systems without atomic modeset support. Falls back when
//! atomic is unavailable.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-impl-device-simple.h
//! Note: Upstream header not found; minimal stub.

/// Simple (non-atomic) KMS implementation for a device
pub struct MetaKmsImplDeviceSimple {
    // TODO: Device state for legacy modeset operations
}

impl MetaKmsImplDeviceSimple {
    /// Create simple KMS implementation for a device
    pub fn new() -> Self {
        MetaKmsImplDeviceSimple {}
    }
}

impl Default for MetaKmsImplDeviceSimple {
    fn default() -> Self {
        Self::new()
    }
}