//! Backlight Ref White ported from GNOME Mutter's src/backends/
//!
//! A backlight implementation that adjusts brightness by modifying the
//! ColorDevice reference luminance factor, used for color-managed displays.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-backlight-ref-white.c

/// A backlight that controls display brightness via reference white adjustments.
/// Communicates with the color manager to set reference luminance factors.
pub struct BacklightRefWhite {
    /// Reference to the monitor this backlight controls
    pub monitor: usize, // Placeholder; upstream uses MetaMonitor pointer
    /// Handle ID for deferred brightness changes
    pub change_ref_white_handle_id: u32,
}

impl BacklightRefWhite {
    /// Create a new reference white backlight for the given backend and monitor.
    pub fn new() -> Self {
        BacklightRefWhite {
            monitor: 0,
            change_ref_white_handle_id: 0,
        }
    }
}

impl Default for BacklightRefWhite {
    fn default() -> Self {
        Self::new()
    }
}
