//! Stream — ported from GNOME Mutter
//!
//! MetaStream is the base class for different types of screen capture streams
//! (window, monitor, area). It provides the interface for creating sources and
//! transforming cursor coordinates.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream.h

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaStreamCursorMode {
    META_STREAM_CURSOR_MODE_HIDDEN = 0,
    META_STREAM_CURSOR_MODE_EMBEDDED = 1,
    META_STREAM_CURSOR_MODE_METADATA = 2,
}

/// MetaStream: Base class for screen capture streams.
pub struct MetaStream {
    // TODO: port remaining fields from upstream meta-stream.c
}

impl MetaStream {
    pub fn new() -> Self {
        MetaStream {}
    }
}

impl Default for MetaStream {
    fn default() -> Self {
        Self::new()
    }
}
