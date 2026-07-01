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
    // TODO: Add remaining fields from MetaMonitor private struct
}

impl Monitor {
    /// Create a new monitor with the given specification
    pub fn new(spec: MonitorSpec) -> Self {
        Monitor { spec }
    }

    /// Get the monitor specification
    pub fn spec(&self) -> &MonitorSpec {
        &self.spec
    }

    // TODO: Add meta_monitor_get_current_resolution
    // TODO: Add meta_monitor_get_physical_dimensions
    // TODO: Add meta_monitor_supports_underscanning
}
