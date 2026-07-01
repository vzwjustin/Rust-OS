//! EDID ported from GNOME Mutter's src/backends/
//!
//! EDID (Extended Display Identification Data) parsing for monitor metadata.
//! Extracts manufacturer code, product code, color primaries, gamma, and HDR capabilities.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/edid.h

use alloc::string::String;

/// MetaEdidInfo — parsed EDID data for a monitor.
/// Contains manufacturer/product identification, color information, and supported video modes.
#[derive(Debug, Clone)]
pub struct MetaEdidInfo {
    /// Manufacturer code (e.g., "AUO" for AU Optronics).
    pub manufacturer_code: Option<String>,
    /// PNP product code.
    pub product_code: u32,
    /// Serial number (if present).
    pub serial_number: u32,
    /// String-encoded serial number (optional).
    pub dsc_serial_number: Option<String>,
    /// Product name from descriptor (optional).
    pub dsc_product_name: Option<String>,
    /// Default color primaries (CIE xy coordinates).
    pub default_color_primaries: [f32; 8], // x1, y1, x2, y2, x3, y3, white_x, white_y
    /// Default gamma (-1.0 if not specified).
    pub default_gamma: f64,
    /// Minimum vertical refresh rate in Hz.
    pub min_vert_rate_hz: i32,
    /// Supported color space info (placeholder).
    pub supports_colorimetry: bool,
    /// HDR static metadata (placeholder).
    pub supports_hdr: bool,
}

impl MetaEdidInfo {
    /// Create a new empty MetaEdidInfo.
    pub fn new() -> Self {
        MetaEdidInfo {
            manufacturer_code: None,
            product_code: 0,
            serial_number: 0,
            dsc_serial_number: None,
            dsc_product_name: None,
            default_color_primaries: [0.0; 8],
            default_gamma: -1.0,
            min_vert_rate_hz: 0,
            supports_colorimetry: false,
            supports_hdr: false,
        }
    }
}

impl Default for MetaEdidInfo {
    fn default() -> Self {
        Self::new()
    }
}
