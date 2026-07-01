//! Stream Monitor — ported from GNOME Mutter
//!
//! MetaStreamMonitor represents a screen capture stream for a single monitor
//! (display output).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-monitor.h

use super::stream::MetaStream;

/// Placeholder for MetaMonitor from monitor manager
pub struct MetaMonitor {
    // TODO: port from upstream
}

/// MetaStreamMonitor: Captures a single monitor's output.
/// Extends MetaStream with monitor and logical monitor references for display capture.
pub struct MetaStreamMonitor {
    pub base: MetaStream,
    /// Monitor reference (opaque MetaMonitor*)
    pub monitor: *mut core::ffi::c_void,
    /// Logical monitor reference (opaque MetaLogicalMonitor*)
    pub logical_monitor: *mut core::ffi::c_void,
}

impl MetaStreamMonitor {
    pub fn new() -> Self {
        MetaStreamMonitor {
            base: MetaStream::new(),
            monitor: core::ptr::null_mut(),
            logical_monitor: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaStreamMonitor {
    fn default() -> Self {
        Self::new()
    }
}
