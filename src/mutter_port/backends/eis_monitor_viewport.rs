//! Eis Monitor Viewport — Logical monitor integration with EIS from GNOME Mutter
//!
//! Maps a MetaLogicalMonitor to an EIS viewport, exposing position, size,
//! physical scale, and coordinate transformation. Implements the EisViewport interface.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-eis-monitor-viewport.h

pub struct MetaLogicalMonitor {
    // Opaque logical monitor type
}

/// MetaEisMonitorViewport — EIS viewport for a logical monitor.
/// Bridges a logical monitor's geometry to the EIS coordinate space.
pub struct MetaEisMonitorViewport {
    pub logical_monitor: *mut MetaLogicalMonitor,
}

impl MetaEisMonitorViewport {
    pub fn new() -> Self {
        MetaEisMonitorViewport {
            logical_monitor: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaEisMonitorViewport {
    fn default() -> Self {
        Self::new()
    }
}
