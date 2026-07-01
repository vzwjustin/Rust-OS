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

    /// Dirty flag set whenever the shape is mutated. The compositor
    /// inspects this to decide whether to recompute the window's
    /// damaged/clip region.
    shape_changed: bool,
}

impl WindowShape {
    /// Create a new window shape tracker.
    pub fn new(xwindow: XWindow) -> Self {
        Self {
            xwindow,
            bounding_region: None,
            input_region: None,
            has_shape: false,
            shape_changed: true,
        }
    }

    /// Update the shape from the X Shape extension.
    ///
    /// A full implementation would call `XShapeQueryExtents` on the X
    /// display to fetch the current bounding and clip rectangles for
    /// `xwindow`, then translate those into region handles via
    /// `XShapeCombineRegion`/`XCreateRegion`. When the extents differ
    /// from the cached values, `bounding_region`/`input_region` are
    /// refreshed and `shape_changed` is raised so the compositor
    /// recomputes the window's damaged region.
    pub fn update(&mut self) {
        // Without an X connection we cannot query the server, so the
        // cached regions remain authoritative. Marking the shape clean
        // here reflects that no server-side change was observed.
        self.shape_changed = false;
    }

    /// Set a custom bounding shape.
    pub fn set_bounding_shape(&mut self, region: Option<u64>) {
        if self.bounding_region != region {
            self.shape_changed = true;
        }
        self.bounding_region = region;
        if region.is_some() {
            self.has_shape = true;
        }
    }

    /// Set a custom input shape.
    pub fn set_input_shape(&mut self, region: Option<u64>) {
        if self.input_region != region {
            self.shape_changed = true;
        }
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

    /// Returns `true` when the shape has been mutated since the last
    /// call to [`clear_dirty`]. The compositor uses this to avoid
    /// recomputing clip/damage regions when nothing changed.
    pub fn is_dirty(&self) -> bool {
        self.shape_changed
    }

    /// Acknowledge that the compositor has consumed the latest shape
    /// state, clearing the dirty flag.
    pub fn clear_dirty(&mut self) {
        self.shape_changed = false;
    }

    /// Reset the shape to rectangular.
    pub fn reset(&mut self) {
        let was_set =
            self.has_shape || self.bounding_region.is_some() || self.input_region.is_some();
        self.bounding_region = None;
        self.input_region = None;
        self.has_shape = false;
        if was_set {
            self.shape_changed = true;
        }
    }
}
