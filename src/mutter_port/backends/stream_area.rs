//! Stream Area — ported from GNOME Mutter
//!
//! MetaStreamArea represents a screen capture stream for a specific rectangular
//! area of the display.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-area.h

use super::stream::MetaStream;

/// Placeholder for MtkRectangle from mtk library
#[derive(Debug, Clone, Copy)]
pub struct MtkRectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// MetaStreamArea: Captures a rectangular area of the display.
pub struct MetaStreamArea {
    base: MetaStream,
    // TODO: area: MtkRectangle,
    // TODO: scale: f32,
}

impl MetaStreamArea {
    pub fn new() -> Self {
        MetaStreamArea {
            base: MetaStream::new(),
        }
    }
}

impl Default for MetaStreamArea {
    fn default() -> Self {
        Self::new()
    }
}
