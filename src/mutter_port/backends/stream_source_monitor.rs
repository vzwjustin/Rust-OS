//! Stream Source Monitor — ported from GNOME Mutter
//!
//! MetaStreamSourceMonitor provides the actual pixel data capture for a monitor-based
//! stream, handling rendering and frame recording for display outputs.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-source-monitor.h

pub struct MetaStreamSource {
    // TODO: port from meta-stream-source.h
}

pub struct MetaStreamMonitor {
    // TODO: port from meta-stream-monitor.h
}

/// MetaStreamSourceMonitor: Pixel source for monitor captures.
pub struct MetaStreamSourceMonitor {
    // TODO: base: MetaStreamSource,
    // TODO: stream_monitor: *mut MetaStreamMonitor,
}

impl MetaStreamSourceMonitor {
    pub fn new() -> Self {
        MetaStreamSourceMonitor {}
    }
}

impl Default for MetaStreamSourceMonitor {
    fn default() -> Self {
        Self::new()
    }
}
