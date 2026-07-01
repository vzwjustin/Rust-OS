//! Window frame decoration management.
//! Ported from src/frames/meta-frame.h/c
use alloc::{format, string::String, vec::Vec};

use super::frame_content::FrameContent;
use super::frame_header::FrameHeader;

/// A window frame decoration for an X11 window.
///
/// Provides window frame decoration rendering and management for traditional
/// window manager frames (title bar, borders, etc.). Interfaces with GTK for
/// the frame widget itself.
#[derive(Debug)]
pub struct Frame {
    /// Border extents (left, right, top, bottom)
    pub extents: (i32, i32, i32, i32),

    /// Window title from _NET_WM_VISIBLE_NAME
    pub visible_name: Option<String>,

    /// Window title from _NET_WM_NAME
    pub name: Option<String>,

    /// Window manager name (WM_NAME)
    pub wm_name: Option<String>,

    /// X11 window ID being decorated
    pub window_id: u32,

    /// Frame content widget (child area).
    pub content_widget: Option<FrameContent>,

    /// Frame header widget (title bar).
    pub header_widget: Option<FrameHeader>,

    /// Whether the window is fullscreen (no decorations).
    pub is_fullscreen: bool,

    /// Whether the window has Motif hints disabling decorations.
    pub motif_no_decorations: bool,

    /// Total window width (including frame).
    pub total_width: i32,

    /// Total window height (including frame).
    pub total_height: i32,
}

impl Frame {
    /// Create a new window frame for the given X11 window.
    pub fn new(window: u32) -> Self {
        Frame {
            extents: (0, 0, 0, 0),
            visible_name: None,
            name: None,
            wm_name: None,
            window_id: window,
            content_widget: None,
            header_widget: None,
            is_fullscreen: false,
            motif_no_decorations: false,
            total_width: 0,
            total_height: 0,
        }
    }

    /// Set the frame content widget.
    pub fn set_content(&mut self, content: FrameContent) {
        self.content_widget = Some(content);
    }

    /// Set the frame header widget.
    pub fn set_header(&mut self, header: FrameHeader) {
        self.header_widget = Some(header);
    }

    /// Handle an X11 event for the frame or its decorated window.
    /// Updates frame state based on property changes.
    pub fn handle_xevent(&mut self, window: u32, event_type: i32, event_data: &[u8]) {
        // Only handle events for our window.
        if window != self.window_id {
            return;
        }
        // event_type 28 = PropertyNotify in X11.
        if event_type == 28 {
            // Property change — a full implementation would check
            // which atom changed and update the corresponding field.
            // _NET_WM_NAME → update self.name
            // _NET_WM_VISIBLE_NAME → update self.visible_name
            // _MOTIF_WM_HINTS → update self.motif_no_decorations
            // _NET_WM_STATE_FULLSCREEN → update self.is_fullscreen
        }
        let _ = event_data;
    }

    /// Get the frame content widget.
    pub fn content(&self) -> Option<&FrameContent> {
        self.content_widget.as_ref()
    }

    /// Get the frame header widget.
    pub fn header(&self) -> Option<&FrameHeader> {
        self.header_widget.as_ref()
    }

    /// Check if frame decorations should be visible. Decorations are
    /// hidden when the window is fullscreen or has Motif hints
    /// requesting no decorations.
    pub fn should_show_decorations(&self) -> bool {
        !self.is_fullscreen && !self.motif_no_decorations
    }

    /// Get the undecorated inner size of the window (client area).
    /// Subtracts the frame extents from the total size.
    pub fn get_client_size(&self) -> (i32, i32) {
        let (left, right, top, bottom) = self.extents;
        let client_w = (self.total_width - left - right).max(0);
        let client_h = (self.total_height - top - bottom).max(0);
        (client_w, client_h)
    }

    /// Set the total window size (including frame).
    pub fn set_total_size(&mut self, width: i32, height: i32) {
        self.total_width = width;
        self.total_height = height;
    }

    /// Set the frame border extents.
    pub fn set_extents(&mut self, left: i32, right: i32, top: i32, bottom: i32) {
        self.extents = (left, right, top, bottom);
    }

    /// Set the fullscreen state.
    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.is_fullscreen = fullscreen;
    }
}
