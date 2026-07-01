//! Kms Plane Private — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-plane-private.h

use alloc::string::String;

/// MetaKmsPlaneProp
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsPlaneProp {
    META_KMS_PLANE_PROP_TYPE = 0,
    META_KMS_PLANE_PROP_ROTATION,
    META_KMS_PLANE_PROP_IN_FORMATS,
    META_KMS_PLANE_PROP_SRC_X,
    META_KMS_PLANE_PROP_SRC_Y,
    META_KMS_PLANE_PROP_SRC_W,
    META_KMS_PLANE_PROP_SRC_H,
    META_KMS_PLANE_PROP_CRTC_X,
    META_KMS_PLANE_PROP_CRTC_Y,
    META_KMS_PLANE_PROP_CRTC_W,
    META_KMS_PLANE_PROP_CRTC_H,
    META_KMS_PLANE_PROP_FB_ID,
    META_KMS_PLANE_PROP_CRTC_ID,
    META_KMS_PLANE_PROP_FB_DAMAGE_CLIPS_ID,
    META_KMS_PLANE_PROP_IN_FENCE_FD,
    META_KMS_PLANE_PROP_HOTSPOT_X,
}

/// MetaKmsPlaneRotationBit
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsPlaneRotationBit {
    META_KMS_PLANE_ROTATION_BIT_ROTATE_0 = 0,
    META_KMS_PLANE_ROTATION_BIT_ROTATE_90,
    META_KMS_PLANE_ROTATION_BIT_ROTATE_180,
    META_KMS_PLANE_ROTATION_BIT_ROTATE_270,
    META_KMS_PLANE_ROTATION_BIT_REFLECT_X,
    META_KMS_PLANE_ROTATION_BIT_REFLECT_Y,
    META_KMS_PLANE_ROTATION_BIT_N_PROPS,
}

// TODO: Extract struct definitions from C header
// TODO: Add type definitions and implementations