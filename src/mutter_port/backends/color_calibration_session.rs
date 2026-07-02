//! Color Calibration Session ported from GNOME Mutter's src/backends/
//!
//! Manages display gamma calibration with 16-bit RGB lookup tables (LUTs).
//! Provides D-Bus session interface for applying color profiles to outputs.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-color-calibration-session.c

use alloc::string::String;
use alloc::vec::Vec;

/// D-Bus Color Manager Calibration skeleton base type (opaque, hardware/D-Bus I/O bound).
pub struct DBusColorManagerCalibrationSkeleton;

/// Color calibration session for display gamma adjustment.
///
/// Manages gamma lookup tables and color profiles. Stores the LUT
/// locally; D-Bus transport for applying to hardware is not available.
pub struct MetaColorCalibrationSession {
    /// Reference to D-Bus skeleton (opaque).
    pub dbus: DBusColorManagerCalibrationSkeleton,
    /// D-Bus object path for this session.
    pub object_path: String,
    /// Current red gamma LUT.
    pub red_lut: Vec<u16>,
    /// Current green gamma LUT.
    pub green_lut: Vec<u16>,
    /// Current blue gamma LUT.
    pub blue_lut: Vec<u16>,
    /// Whether a gamma LUT has been applied.
    pub gamma_applied: bool,
}

impl MetaColorCalibrationSession {
    /// Create a new color calibration session with the given D-Bus object path.
    pub fn new(object_path: &str) -> Self {
        MetaColorCalibrationSession {
            dbus: DBusColorManagerCalibrationSkeleton,
            object_path: String::from(object_path),
            red_lut: Vec::new(),
            green_lut: Vec::new(),
            blue_lut: Vec::new(),
            gamma_applied: false,
        }
    }

    /// Get the D-Bus object path for this session.
    pub fn get_object_path(&self) -> &str {
        &self.object_path
    }

    /// Apply gamma calibration LUT. Stores the LUT locally. A full
    /// implementation would write the LUT to the display hardware via
    /// DRM gamma_lut property or D-Bus color manager interface.
    pub fn apply_gamma(
        &mut self,
        red: &[u16],
        green: &[u16],
        blue: &[u16],
    ) -> Result<(), &'static str> {
        // Validate LUT sizes match.
        if red.len() != green.len() || green.len() != blue.len() {
            return Err("gamma LUT size mismatch");
        }
        self.red_lut = red.to_vec();
        self.green_lut = green.to_vec();
        self.blue_lut = blue.to_vec();
        self.gamma_applied = true;
        Ok(())
    }

    /// Get the current red gamma LUT.
    pub fn get_red_lut(&self) -> &[u16] {
        &self.red_lut
    }

    /// Get the current green gamma LUT.
    pub fn get_green_lut(&self) -> &[u16] {
        &self.green_lut
    }

    /// Get the current blue gamma LUT.
    pub fn get_blue_lut(&self) -> &[u16] {
        &self.blue_lut
    }

    /// Whether a gamma LUT has been applied.
    pub fn is_gamma_applied(&self) -> bool {
        self.gamma_applied
    }
}

impl Default for MetaColorCalibrationSession {
    fn default() -> Self {
        Self::new("/org/gnome/Mutter/ColorCalibration/Session0")
    }
}
