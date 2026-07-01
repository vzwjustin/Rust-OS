//! Kms Crtc Private — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-crtc-private.h

use alloc::string::String;

/// MetaKmsCrtcProp
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsCrtcProp {
    META_KMS_CRTC_PROP_MODE_ID = 0,
    META_KMS_CRTC_PROP_ACTIVE,
    META_KMS_CRTC_PROP_DEGAMMA_LUT,
    META_KMS_CRTC_PROP_DEGAMMA_LUT_SIZE,
    META_KMS_CRTC_PROP_CTM,
    META_KMS_CRTC_PROP_GAMMA_LUT,
    META_KMS_CRTC_PROP_GAMMA_LUT_SIZE,
    META_KMS_CRTC_PROP_VRR_ENABLED,
    META_KMS_CRTC_N_PROPS,
}

// TODO: Extract struct definitions from C header
// TODO: Add type definitions and implementations