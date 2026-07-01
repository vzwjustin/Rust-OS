//! Frame header widget - the title bar and decorations of a window frame.
//! Ported from src/frames/meta-frame-header.h/c
use alloc::{format, string::String, vec::Vec};

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
    /// Whether the header needs redraw.
    pub needs_redraw: bool,
}

impl FrameHeader {
    /// Create a new frame header widget.
    pub fn new() -> Self {
        FrameHeader {
            title: None,
            focused: false,
            needs_redraw: true,
        }
    }

    /// Set the window title displayed in the header. Marks the header
    /// for redraw when the title changes.
    pub fn set_title(&mut self, title: Option<String>) {
        if self.title != title {
            self.title = title;
            self.needs_redraw = true;
        }
    }

    /// Set whether the frame is focused (highlighted). Marks the header
    /// for redraw when focus state changes.
    pub fn set_focused(&mut self, focused: bool) {
        if self.focused != focused {
            self.focused = focused;
            self.needs_redraw = true;
        }
    }

    /// Trigger a redraw of the header. Marks the needs_redraw flag.
    pub fn queue_draw(&mut self) {
        self.needs_redraw = true;
    }

    /// Check if the header needs redraw.
    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    /// Clear the redraw flag (after painting).
    pub fn clear_redraw(&mut self) {
        self.needs_redraw = false;
    }
}

impl Default for FrameHeader {
    fn default() -> Self {
        Self::new()
    }
}
