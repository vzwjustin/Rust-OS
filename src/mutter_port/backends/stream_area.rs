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
    /// The rectangular capture area.
    pub area: MtkRectangle,
    /// Scale factor for the captured area.
    pub scale: f32,
}

impl MetaStreamArea {
    pub fn new() -> Self {
        MetaStreamArea {
            base: MetaStream::new(),
            area: MtkRectangle {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            scale: 1.0,
        }
    }

    /// Set the capture area.
    pub fn set_area(&mut self, area: MtkRectangle) {
        self.area = area;
    }

    /// Get the capture area.
    pub fn get_area(&self) -> MtkRectangle {
        self.area
    }

    /// Set the scale factor.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    /// Get the scale factor.
    pub fn get_scale(&self) -> f32 {
        self.scale
    }
}

impl Default for MetaStreamArea {
    fn default() -> Self {
        Self::new()
    }
}
