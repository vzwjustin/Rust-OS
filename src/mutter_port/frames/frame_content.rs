//! Frame content widget - the client window area within the decorated frame.
//! Ported from src/frames/meta-frame-content.h/c

/// A widget representing the content area of a decorated window.
///
/// This is the area that contains the actual client window content,
/// separate from the frame decorations.
#[derive(Debug)]
pub struct FrameContent {
    /// X11 window ID of the embedded client window
    pub window_id: u32,
    /// Whether the content is currently visible.
    pub visible: bool,
    /// Content width in pixels.
    pub width: i32,
    /// Content height in pixels.
    pub height: i32,
}

impl FrameContent {
    /// Create a new frame content widget for an X11 window. A full
    /// implementation would create a GTK widget, set up XEmbed
    /// window embedding, and connect to window events.
    pub fn new(window: u32) -> Self {
        FrameContent {
            window_id: window,
            visible: false,
            width: 0,
            height: 0,
        }
    }

    /// Get the X11 window ID embedded in this content widget.
    pub fn get_window(&self) -> u32 {
        self.window_id
    }

    /// Set the content visibility.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Whether the content is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Set the content size.
    pub fn set_size(&mut self, width: i32, height: i32) {
        self.width = width;
        self.height = height;
    }

    /// Get the content size (width, height).
    pub fn get_size(&self) -> (i32, i32) {
        (self.width, self.height)
    }
}
