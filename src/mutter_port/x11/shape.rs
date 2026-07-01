//! X11 window shape support (X Shape extension).
//!
//! Ported from GNOME Mutter's src/x11/meta-window-shape.c/.h.
//! Handles non-rectangular window shapes via the X Shape extension.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-window-shape.c

use crate::mutter_port::x11::display::XWindow;

/// Represents a window shape region.
#[derive(Debug, Clone)]
pub struct WindowShape {
    pub xwindow: XWindow,

    /// Bounding shape region (determines window extent).
    pub bounding_region: Option<u64>,

    /// Input shape region (determines clickable area).
    pub input_region: Option<u64>,

    /// Whether this window has a non-rectangular shape.
    pub has_shape: bool,
}

impl WindowShape {
    /// Create a new window shape tracker.
    /// # TODO: port logic from meta_window_shape_new()
    pub fn new(xwindow: XWindow) -> Self {
        Self {
            xwindow,
            bounding_region: None,
            input_region: None,
            has_shape: false,
        }
    }

    /// Update the shape from the X Shape extension.
    /// # TODO: port logic from meta_window_shape_update()
    pub fn update(&mut self) {
        // TODO: call XShapeGetRectangles or XShapeQueryExtents
    }

    /// Set a custom bounding shape.
    /// # TODO: port logic from XShapeCombineRegion
    pub fn set_bounding_shape(&mut self, region: Option<u64>) {
        self.bounding_region = region;
        if region.is_some() {
            self.has_shape = true;
        }
    }

    /// Set a custom input shape.
    /// # TODO: port logic from XShapeCombineRegion
    pub fn set_input_shape(&mut self, region: Option<u64>) {
        self.input_region = region;
    }

    /// Get the bounding shape region.
    pub fn get_bounding_region(&self) -> Option<u64> {
        self.bounding_region
    }

    /// Get the input shape region.
    pub fn get_input_region(&self) -> Option<u64> {
        self.input_region
    }

    /// Check if this window has a non-rectangular shape.
    pub fn has_custom_shape(&self) -> bool {
        self.has_shape
    }

    /// Reset the shape to rectangular.
    /// # TODO: port logic from meta_window_shape_reset()
    pub fn reset(&mut self) {
        self.bounding_region = None;
        self.input_region = None;
        self.has_shape = false;
    }
}
