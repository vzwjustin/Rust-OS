//! Stream Virtual — ported from GNOME Mutter
//!
//! MetaStreamVirtual represents a screen capture stream for a virtual (software-defined)
//! monitor. Used for capturing output from headless or remote display scenarios.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-virtual.h

use super::stream::MetaStream;

pub struct MetaVirtualMonitor {
    // TODO: port from upstream
}

/// MetaStreamVirtual: Captures a virtual monitor's output.
pub struct MetaStreamVirtual {
    base: MetaStream,
    // TODO: virtual_monitor: *mut MetaVirtualMonitor,
}

impl MetaStreamVirtual {
    pub fn new() -> Self {
        MetaStreamVirtual {
            base: MetaStream::new(),
        }
    }
}

impl Default for MetaStreamVirtual {
    fn default() -> Self {
        Self::new()
    }
}
