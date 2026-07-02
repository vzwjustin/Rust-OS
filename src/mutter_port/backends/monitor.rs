//! Monitor types — ported from GNOME Mutter's `src/backends/meta-monitor-private.h`
//!
//! Represents display monitors with their specifications, modes, and configuration.

use alloc::string::String;

/// MetaMonitorSpec — identifiers for a monitor
#[derive(Debug, Clone)]
pub struct MonitorSpec {
    pub connector: String,
    pub vendor: String,
    pub product: String,
    pub serial: String,
}

/// MetaMonitorModeSpec — display mode specifications
#[derive(Debug, Clone, Copy)]
pub struct MonitorModeSpec {
    pub width: u32,
    pub height: u32,
    pub refresh_rate: f32,
}

/// MetaMonitor — a physical display device
#[derive(Debug, Clone)]
pub struct Monitor {
    spec: MonitorSpec,
    /// Current display mode (width, height, refresh rate).
    current_mode: Option<MonitorModeSpec>,
    /// Physical dimensions in millimeters.
    physical_width_mm: u32,
    /// Physical height in millimeters.
    physical_height_mm: u32,
    /// Whether this monitor supports underscanning.
    supports_underscanning: bool,
}

impl Monitor {
    /// Create a new monitor with the given specification
    pub fn new(spec: MonitorSpec) -> Self {
        Monitor {
            spec,
            current_mode: None,
            physical_width_mm: 0,
            physical_height_mm: 0,
            supports_underscanning: false,
        }
    }

    /// Get the monitor specification
    pub fn spec(&self) -> &MonitorSpec {
        &self.spec
    }

    /// Set the current display mode.
    pub fn set_current_mode(&mut self, mode: MonitorModeSpec) {
        self.current_mode = Some(mode);
    }

    /// Get the current resolution as (width, height, refresh_rate).
    pub fn get_current_resolution(&self) -> Option<(u32, u32, f32)> {
        self.current_mode
            .as_ref()
            .map(|m| (m.width, m.height, m.refresh_rate))
    }

    /// Set physical dimensions in millimeters.
    pub fn set_physical_dimensions(&mut self, width_mm: u32, height_mm: u32) {
        self.physical_width_mm = width_mm;
        self.physical_height_mm = height_mm;
    }

    /// Get physical dimensions in millimeters as (width, height).
    pub fn get_physical_dimensions(&self) -> (u32, u32) {
        (self.physical_width_mm, self.physical_height_mm)
    }

    /// Set whether this monitor supports underscanning.
    pub fn set_supports_underscanning(&mut self, supports: bool) {
        self.supports_underscanning = supports;
    }

    /// Check if this monitor supports underscanning.
    pub fn supports_underscanning(&self) -> bool {
        self.supports_underscanning
    }
}
