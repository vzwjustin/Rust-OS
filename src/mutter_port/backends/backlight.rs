//! Backlight ported from GNOME Mutter's src/backends/
//!
//! Display backlight brightness control abstraction. Provides async brightness
//! adjustment with min/max info and change tracking for hardware backlights.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/meta/meta-backlight.h

/// MetaBacklight — a G_DECLARE_DERIVABLE_TYPE for monitor backlight control.
/// Opaque struct; real implementation in C backend (DRM, sysfs, or platform-specific).
pub struct MetaBacklight;

impl MetaBacklight {
    /// Create a new MetaBacklight (stub).
    pub fn new() -> Self {
        MetaBacklight
    }
}

impl Default for MetaBacklight {
    fn default() -> Self {
        Self::new()
    }
}
