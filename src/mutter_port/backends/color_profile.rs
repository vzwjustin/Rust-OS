//! Color Profile ported from GNOME Mutter's src/backends/
//!
//! ICC color profile management. Handles VCGT (tone curves) and color adaptation
//! matrices for color correction. Wraps colord and LCMS2 for profile loading and gamma LUT generation.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-color-profile.h

use alloc::string::String;

/// MetaColorCalibration — calibration data for a color profile.
/// Contains VCGT tone curves, color adaptation matrix, and brightness profile path.
#[derive(Debug, Clone)]
pub struct MetaColorCalibration {
    /// Whether VCGT (tone curves) are present.
    pub has_vcgt: bool,
    /// VCGT tone curves (placeholder; real impl uses cmsToneCurve from LCMS).
    pub vcgt: [u32; 3],
    /// Whether color adaptation matrix is present.
    pub has_adaptation_matrix: bool,
    /// 3x3 color adaptation matrix (flattened placeholder).
    pub adaptation_matrix: [f32; 9],
    /// Path to brightness profile (optional).
    pub brightness_profile: Option<String>,
}

impl MetaColorCalibration {
    /// Create a new empty calibration.
    pub fn new() -> Self {
        MetaColorCalibration {
            has_vcgt: false,
            vcgt: [0; 3],
            has_adaptation_matrix: false,
            adaptation_matrix: [0.0; 9],
            brightness_profile: None,
        }
    }
}

impl Default for MetaColorCalibration {
    fn default() -> Self {
        Self::new()
    }
}

/// MetaColorProfile — a G_DECLARE_FINAL_TYPE for ICC color profiles.
/// Opaque stub; real implementation in C backend.
pub struct MetaColorProfile;

impl MetaColorProfile {
    /// Create a new MetaColorProfile (stub).
    pub fn new() -> Self {
        MetaColorProfile
    }
}

impl Default for MetaColorProfile {
    fn default() -> Self {
        Self::new()
    }
}
