//! Kms Types — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-types.h

use alloc::string::String;

/// MetaKmsDeviceFlag
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsDeviceFlag {
    META_KMS_DEVICE_FLAG_NONE = 0,
    META_KMS_DEVICE_FLAG_BOOT_VGA = 1 << 0,
    META_KMS_DEVICE_FLAG_PLATFORM_DEVICE = 1 << 1,
    META_KMS_DEVICE_FLAG_DISABLE_MODIFIERS = 1 << 2,
    META_KMS_DEVICE_FLAG_PREFERRED_PRIMARY = 1 << 3,
    META_KMS_DEVICE_FLAG_NO_MODE_SETTING = 1 << 4,
    META_KMS_DEVICE_FLAG_HAS_ADDFB2 = 1 << 5,
    META_KMS_DEVICE_FLAG_DISABLE_CLIENT_MODIFIERS = 1 << 6,
    META_KMS_DEVICE_FLAG_SUPPORTS_COLOR_MODES = 1 << 7,
}

/// MetaKmsResourceChanges
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsResourceChanges {
    META_KMS_RESOURCE_CHANGE_NONE = 0,
    META_KMS_RESOURCE_CHANGE_CRTC_COLOR_PIPELINE = 1 << 0,
    META_KMS_RESOURCE_CHANGE_NO_DEVICES = 1 << 1,
    META_KMS_RESOURCE_CHANGE_PRIVACY_SCREEN = 1 << 2,
    META_KMS_RESOURCE_CHANGE_FULL = 0xFFFFFFFF,
}
