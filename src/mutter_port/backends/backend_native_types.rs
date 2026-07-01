//! Backend Native Types ported from GNOME Mutter's src/backends/
//!
//! Type definitions for native backend subsystems including DRM, KMS, and seat management.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-backend-native-types.h
//! Upstream header not found; minimal stub.

use core::ffi::c_void;

/// Opaque native backend instance.
pub struct MetaBackendNative {
    _phantom: core::marker::PhantomData<c_void>,
}

impl MetaBackendNative {
    /// Create a new native backend.
    pub fn new() -> Self {
        MetaBackendNative {
            _phantom: core::marker::PhantomData,
        }
    }
}

impl Default for MetaBackendNative {
    fn default() -> Self {
        Self::new()
    }
}

/// Opaque DRM device.
pub struct MetaDrmDevice;

/// Opaque KMS connector.
pub struct MetaKmsConnector;

/// Opaque KMS plane.
pub struct MetaKmsPlane;

/// Opaque KMS CRTC.
pub struct MetaKmsCrtc;

/// Opaque seat native.
pub struct MetaSeatNative;

/// Opaque virtual monitor.
pub struct MetaVirtualMonitorNative;
