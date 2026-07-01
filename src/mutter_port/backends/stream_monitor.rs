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
pub struct MetaStreamMonitor {
    base: MetaStream,
    // TODO: monitor: *mut MetaMonitor,
}

impl MetaStreamMonitor {
    pub fn new() -> Self {
        MetaStreamMonitor {
            base: MetaStream::new(),
        }
    }
}

impl Default for MetaStreamMonitor {
    fn default() -> Self {
        Self::new()
    }
}
