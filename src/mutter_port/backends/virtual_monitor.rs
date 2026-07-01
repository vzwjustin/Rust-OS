//! Virtual Monitor — ported from GNOME Mutter
//!
//! Virtual monitor mode and configuration management.
//! Manages virtual display modes and EDID information (vendor, product, serial).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-virtual-monitor.h

use alloc::string::String;

/// Virtual monitor mode information.
#[derive(Debug, Clone)]
pub struct MetaVirtualModeInfo {
    pub width: i32,
    pub height: i32,
    pub refresh_rate: f32,
    pub has_preferred_scale: bool,
    pub preferred_scale: f32,
}

impl MetaVirtualModeInfo {
    /// Create a new virtual mode with width, height, and refresh rate.
    pub fn new(width: i32, height: i32, refresh_rate: f32) -> Self {
        MetaVirtualModeInfo {
            width,
            height,
            refresh_rate,
            has_preferred_scale: false,
            preferred_scale: 1.0,
        }
    }

    /// Check if mode is valid (positive width and height).
    pub fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }

    /// Set the preferred scale for this mode.
    pub fn set_preferred_scale(&mut self, scale: f32) {
        self.preferred_scale = scale;
        self.has_preferred_scale = true;
    }
}

/// Virtual monitor information with EDID and mode list.
#[derive(Debug, Clone)]
pub struct MetaVirtualMonitorInfo {
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub serial: Option<String>,
    // mode_infos would be Vec<MetaVirtualModeInfo> but kept opaque for now
}

impl MetaVirtualMonitorInfo {
    /// Create a new virtual monitor with vendor, product, and serial identifiers.
    pub fn new(vendor: Option<String>, product: Option<String>, serial: Option<String>) -> Self {
        MetaVirtualMonitorInfo {
            vendor,
            product,
            serial,
        }
    }

    /// Create a simple virtual monitor with a single mode.
    pub fn new_simple(
        width: i32,
        height: i32,
        refresh_rate: f32,
        vendor: Option<String>,
        product: Option<String>,
        serial: Option<String>,
    ) -> Self {
        MetaVirtualMonitorInfo {
            vendor,
            product,
            serial,
        }
    }

    /// Create an inactive virtual monitor (no modes).
    pub fn new_inactive(
        vendor: Option<String>,
        product: Option<String>,
        serial: Option<String>,
    ) -> Self {
        MetaVirtualMonitorInfo {
            vendor,
            product,
            serial,
        }
    }
}

impl Default for MetaVirtualModeInfo {
    fn default() -> Self {
        Self::new(1024, 768, 60.0)
    }
}

impl Default for MetaVirtualMonitorInfo {
    fn default() -> Self {
        Self::new(None, None, None)
    }
}

/// Virtual monitor object.
pub struct MetaVirtualMonitor;

impl MetaVirtualMonitor {
    pub fn new() -> Self {
        MetaVirtualMonitor
    }
}

impl Default for MetaVirtualMonitor {
    fn default() -> Self {
        Self::new()
    }
}