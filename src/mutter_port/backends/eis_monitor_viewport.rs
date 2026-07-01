//! Eis Monitor Viewport — Logical monitor integration with EIS from GNOME Mutter
//!
//! Maps a MetaLogicalMonitor to an EIS viewport, exposing position, size,
//! physical scale, and coordinate transformation. Implements the EisViewport interface.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-eis-monitor-viewport.h

/// MetaEisMonitorViewport — EIS viewport for a logical monitor.
/// Bridges a logical monitor's geometry to the EIS coordinate space.
pub struct MetaEisMonitorViewport {
    // TODO: port fields from meta-eis-monitor-viewport.c
}

impl MetaEisMonitorViewport {
    pub fn new() -> Self {
        MetaEisMonitorViewport {}
    }
}

impl Default for MetaEisMonitorViewport {
    fn default() -> Self {
        Self::new()
    }
}
