//! Frame header widget - the title bar and decorations of a window frame.
//! Ported from src/frames/meta-frame-header.h/c
use alloc::{string::String, vec::Vec, format};

/// A widget representing the header/title bar of a decorated window.
///
/// This is the top part of a window frame that typically contains
/// the window title, minimize/maximize/close buttons, and other controls.
#[derive(Debug)]
pub struct FrameHeader {
    /// Window title to display
    pub title: Option<String>,
    /// Whether the window is currently focused
    pub focused: bool,
}

impl FrameHeader {
    /// Create a new frame header widget.
    ///
    /// # TODO
    /// Port logic from meta_frame_header_new:
    /// - Create GTK widget
    /// - Set up title label
    /// - Create window control buttons (minimize, maximize, close)
    /// - Set up CSS styling for appearance
    pub fn new() -> Self {
        FrameHeader {
            title: None,
            focused: false,
        }
    }

    /// Set the window title displayed in the header.
    pub fn set_title(&mut self, title: Option<String>) {
        self.title = title;
        // TODO: update widget display
    }

    /// Set whether the frame is focused (highlighted).
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        // TODO: update styling based on focus state
    }

    /// Trigger a redraw of the header.
    pub fn queue_draw(&mut self) {
        // TODO: request GTK widget redraw
    }
}

impl Default for FrameHeader {
    fn default() -> Self {
        Self::new()
    }
}
