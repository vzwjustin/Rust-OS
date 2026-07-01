//! Stream Source Area — ported from GNOME Mutter
//!
//! MetaStreamSourceArea provides the actual pixel data capture for an area-based
//! stream, handling rendering and frame recording for a rectangular region.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-source-area.h

pub struct MetaStreamSource {
    // TODO: port from meta-stream-source.h
}

/// MetaStreamSourceArea: Pixel source for area captures.
pub struct MetaStreamSourceArea {
    // TODO: base: MetaStreamSource,
    // TODO: stream_area: *mut MetaStreamArea,
}

impl MetaStreamSourceArea {
    pub fn new() -> Self {
        MetaStreamSourceArea {}
    }
}

impl Default for MetaStreamSourceArea {
    fn default() -> Self {
        Self::new()
    }
}
