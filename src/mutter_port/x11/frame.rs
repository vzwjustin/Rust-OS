//! X11 window frame management.
//!
//! Ported from GNOME Mutter's src/x11/meta-x11-frame.c/.h.
//! Manages client-side decorations (CSD) frames with window borders and title bars.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-x11-frame.c

use crate::mutter_port::x11::display::XWindow;

/// Opaque frame handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameId(pub u64);

/// Represents a window frame with decorations.
pub struct MetaX11Frame {
    pub frame_id: FrameId,
    pub xwindow: XWindow,

    /// Dimensions of the frame.
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,

    /// Border widths.
    pub left_width: i32,
    pub right_width: i32,
    pub top_height: i32,
    pub bottom_height: i32,

    /// Title bar height.
    pub title_height: i32,

    /// Flags for frame state.
    pub is_shaded: bool,
    pub has_focus: bool,

    /// Whether the frame decorations need repainting.
    pub needs_repaint: bool,
}

impl MetaX11Frame {
    /// Create a new frame for an X window.
    pub fn new(xwindow: XWindow) -> Self {
        Self {
            frame_id: FrameId(0),
            xwindow,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            left_width: 0,
            right_width: 0,
            top_height: 0,
            bottom_height: 0,
            title_height: 0,
            is_shaded: false,
            has_focus: false,
            needs_repaint: true,
        }
    }

    /// Set the frame position and size.
    pub fn set_size(&mut self, x: i32, y: i32, width: i32, height: i32) {
        if self.x != x || self.y != y || self.width != width || self.height != height {
            self.x = x;
            self.y = y;
            self.width = width;
            self.height = height;
            self.needs_repaint = true;
        }
    }

    /// Mark the frame as needing a repaint.
    pub fn invalidate(&mut self) {
        self.needs_repaint = true;
    }

    /// Update the frame appearance based on window state.
    ///
    /// A full implementation would render the title bar, borders, and buttons
    /// into the frame pixmap via Cairo and push it to the X server. Here we
    /// track the dirty flag: if the frame is already clean this is a no-op,
    /// otherwise the caller is expected to perform the actual draw and then
    /// call `mark_clean`. Returns true if a repaint was required.
    pub fn repaint(&mut self) -> bool {
        if self.needs_repaint {
            // A full implementation would:
            //  1. Compute the exposed frame geometry (respecting is_shaded,
            //     which collapses the window to just the title bar).
            //  2. Draw the title bar text and buttons with the focus-aware
            //     colors (has_focus selects active vs inactive theme).
            //  3. Composite the result onto the frame window via XPutImage /
            //     Cairo, then flush the X connection.
            // The dirty flag is left set so callers can observe that work is
            // pending; they clear it with `mark_clean` once drawing is done.
            true
        } else {
            false
        }
    }

    /// Clear the repaint-required flag after the frame has been drawn.
    pub fn mark_clean(&mut self) {
        self.needs_repaint = false;
    }

    /// Set the frame's border widths.
    pub fn set_borders(&mut self, left: i32, right: i32, top: i32, bottom: i32, title: i32) {
        if self.left_width != left
            || self.right_width != right
            || self.top_height != top
            || self.bottom_height != bottom
            || self.title_height != title
        {
            self.left_width = left;
            self.right_width = right;
            self.top_height = top;
            self.bottom_height = bottom;
            self.title_height = title;
            self.needs_repaint = true;
        }
    }

    /// Set focus state and repaint.
    pub fn focus_changed(&mut self, has_focus: bool) {
        if self.has_focus != has_focus {
            self.has_focus = has_focus;
            self.needs_repaint = true;
        }
        self.repaint();
    }

    /// Shade the window (collapse to title bar).
    pub fn set_shaded(&mut self, shaded: bool) {
        if self.is_shaded != shaded {
            self.is_shaded = shaded;
            self.needs_repaint = true;
        }
        self.repaint();
    }

    /// Check if a point is in the window frame decorations.
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// Get the client area rectangle (excluding frame).
    pub fn get_client_rect(&self) -> (i32, i32, i32, i32) {
        (
            self.x + self.left_width,
            self.y + self.top_height,
            self.width - self.left_width - self.right_width,
            self.height - self.top_height - self.bottom_height,
        )
    }
}
