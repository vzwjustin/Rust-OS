//! Stream Window — ported from GNOME Mutter
//!
//! MetaStreamWindow represents a screen capture stream for a single application window.
//! It captures the contents of a specific window and any child windows.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-window.h

use super::stream::MetaStream;

pub struct MetaWindow {
    // Opaque window object from meta/window.h
}

/// MetaStreamWindow: Captures a single application window.
pub struct MetaStreamWindow {
    base: MetaStream,
    // TODO: window: *mut MetaWindow,
}

impl MetaStreamWindow {
    pub fn new() -> Self {
        MetaStreamWindow {
            base: MetaStream::new(),
        }
    }
}

impl Default for MetaStreamWindow {
    fn default() -> Self {
        Self::new()
    }
}
