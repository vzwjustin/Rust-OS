//! Kms Plane Private — ported from GNOME Mutter
//!
//! KMS plane properties and rotation/transformation support.
//! Planes are display composition units that can transform and position framebuffers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-kms-plane-private.h

use alloc::string::String;

/// KMS plane property identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsPlaneProp {
    TYPE = 0,
    ROTATION = 1,
    IN_FORMATS = 2,
    SRC_X = 3,
    SRC_Y = 4,
    SRC_W = 5,
    SRC_H = 6,
    CRTC_X = 7,
    CRTC_Y = 8,
    CRTC_W = 9,
    CRTC_H = 10,
    FB_ID = 11,
    CRTC_ID = 12,
    FB_DAMAGE_CLIPS_ID = 13,
    IN_FENCE_FD = 14,
    HOTSPOT_X = 15,
    HOTSPOT_Y = 16,
    SIZE_HINTS = 17,
    YCBCR_COLOR_ENCODING = 18,
    YCBCR_COLOR_RANGE = 19,
    N_PROPS = 20,
}

/// Plane rotation/flip bit indices (enum variant, not bitmask).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsPlaneRotationBit {
    ROTATE_0 = 0,
    ROTATE_90 = 1,
    ROTATE_180 = 2,
    ROTATE_270 = 3,
    REFLECT_X = 4,
    REFLECT_Y = 5,
    N_PROPS = 6,
}

/// Plane rotation/flip as bitmask values (power-of-2 flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsPlaneRotation {
    ROTATE_0 = (1 << 0),
    ROTATE_90 = (1 << 1),
    ROTATE_180 = (1 << 2),
    ROTATE_270 = (1 << 3),
    REFLECT_X = (1 << 4),
    REFLECT_Y = (1 << 5),
    UNKNOWN = (1 << 6),
}

/// Number of plane properties.
pub const META_KMS_PLANE_N_PROPS: u32 = 20;