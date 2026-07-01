//! Stream Source Window — ported from GNOME Mutter
//!
//! MetaStreamSourceWindow provides the actual pixel data capture for a window-based
//! stream, handling rendering and frame recording for individual application windows.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-source-window.h

pub struct MetaStreamSource {
    // TODO: port from meta-stream-source.h
}

pub struct MetaStreamWindow {
    // TODO: port from meta-stream-window.h
}

/// MetaStreamSourceWindow: Pixel source for window captures.
pub struct MetaStreamSourceWindow {
    // TODO: base: MetaStreamSource,
    // TODO: stream_window: *mut MetaStreamWindow,
}

impl MetaStreamSourceWindow {
    pub fn new() -> Self {
        MetaStreamSourceWindow {}
    }
}

impl Default for MetaStreamSourceWindow {
    fn default() -> Self {
        Self::new()
    }
}
