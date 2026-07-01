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
}

impl MetaX11Frame {
    /// Create a new frame for an X window.
    /// # TODO: port logic from meta_x11_frame_new()
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
        }
    }

    /// Set the frame position and size.
    /// # TODO: port logic from meta_x11_frame_set_size()
    pub fn set_size(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.x = x;
        self.y = y;
        self.width = width;
        self.height = height;
    }

    /// Update the frame appearance based on window state.
    /// # TODO: port logic from meta_x11_frame_repaint()
    pub fn repaint(&self) {
        // TODO: repaint frame decorations
    }

    /// Set the frame's border widths.
    /// # TODO: port logic from meta_x11_frame_set_borders()
    pub fn set_borders(
        &mut self,
        left: i32,
        right: i32,
        top: i32,
        bottom: i32,
        title: i32,
    ) {
        self.left_width = left;
        self.right_width = right;
        self.top_height = top;
        self.bottom_height = bottom;
        self.title_height = title;
    }

    /// Set focus state and repaint.
    /// # TODO: port logic from meta_x11_frame_focus_changed()
    pub fn focus_changed(&mut self, has_focus: bool) {
        self.has_focus = has_focus;
        self.repaint();
    }

    /// Shade the window (collapse to title bar).
    /// # TODO: port logic from frame shading
    pub fn set_shaded(&mut self, shaded: bool) {
        self.is_shaded = shaded;
        self.repaint();
    }

    /// Check if a point is in the window frame decorations.
    /// # TODO: port logic from meta_x11_frame_contains_point()
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.x
            && x < self.x + self.width
            && y >= self.y
            && y < self.y + self.height
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
