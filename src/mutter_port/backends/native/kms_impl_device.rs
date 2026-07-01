//! Kms Impl Device — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-impl-device.h

use alloc::string::String;

/// MetaKmsDeviceCaps
#[derive(Debug, Clone)]
pub struct MetaKmsDeviceCaps {
    pub has_cursor_size: bool,
    pub cursor_width: u32,
    pub cursor_height: u32,
    pub prefers_shadow_buffer: bool,
    pub uses_monotonic_clock: bool,
    pub addfb2_modifiers: bool,
    pub supports_color_modes: bool,
}

impl MetaKmsDeviceCaps {
    /// Get the KMS implementation device handle.
    /// Without a real DRM device, returns None.
    pub fn kms_impl_device_get_impl(&self) -> Option<()> {
        None
    }

    /// Get the underlying DRM device path.
    /// Without a real DRM device, returns None.
    pub fn kms_impl_device_get_device(&self) -> Option<()> {
        None
    }
}

/// MetaKmsEnum
#[derive(Debug, Clone)]
pub struct MetaKmsEnum {
    pub valid: bool,
    pub value: u32,
    pub bitmask: u32,
}

impl MetaKmsEnum {
    /// Get the KMS implementation device handle.
    /// Without a real DRM device, returns None.
    pub fn kms_impl_device_get_impl(&self) -> Option<()> {
        None
    }

    /// Get the underlying DRM device path.
    /// Without a real DRM device, returns None.
    pub fn kms_impl_device_get_device(&self) -> Option<()> {
        None
    }
}
