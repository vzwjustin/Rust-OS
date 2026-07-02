//! Kms Crtc Private — ported from GNOME Mutter
//!
//! KMS CRTC properties and state management.
//! Maps DRM property IDs to high-level CRTC configuration attributes.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-kms-crtc-private.h

use alloc::string::String;

/// KMS CRTC property identifiers.
/// Maps kernel DRM properties to logical CRTC configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsCrtcProp {
    /// Display mode blob ID.
    MODE_ID = 0,
    /// CRTC active state.
    ACTIVE = 1,
    /// Degamma LUT blob ID.
    DEGAMMA_LUT = 2,
    /// Degamma LUT entry count.
    DEGAMMA_LUT_SIZE = 3,
    /// Color transform matrix blob ID.
    CTM = 4,
    /// Gamma LUT blob ID.
    GAMMA_LUT = 5,
    /// Gamma LUT entry count.
    GAMMA_LUT_SIZE = 6,
    /// Variable refresh rate enabled.
    VRR_ENABLED = 7,
    /// Number of CRTC properties.
    N_PROPS = 8,
}

/// Number of CRTC properties.
pub const META_KMS_CRTC_N_PROPS: u32 = 8;
