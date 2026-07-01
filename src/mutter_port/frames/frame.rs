//! Window frame decoration management.
//! Ported from src/frames/meta-frame.h/c
use alloc::{string::String, vec::Vec, format};

use super::frame_content::FrameContent;
use super::frame_header::FrameHeader;

/// A window frame decoration for an X11 window.
///
/// Provides window frame decoration rendering and management for traditional
/// window manager frames (title bar, borders, etc.). Interfaces with GTK for
/// the frame widget itself.
#[derive(Debug)]
pub struct Frame {
    // TODO: port GtkWindow parent_instance field (widget state)
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

    // TODO: port Atom fields for X11 property atoms
    // Atom atom__NET_WM_VISIBLE_NAME;
    // Atom atom__NET_WM_NAME;
    // Atom atom__MOTIF_WM_HINTS;
    // Atom atom__NET_WM_STATE;
    // Atom atom__NET_WM_STATE_FULLSCREEN;
}

impl Frame {
    /// Create a new window frame for the given X11 window.
    ///
    /// # Arguments
    /// * `window` - X11 window ID to decorate
    ///
    /// # TODO
    /// Port logic from meta_frame_new - create GTK widget hierarchy,
    /// set up frame content and header, initialize property atoms
    pub fn new(window: u32) -> Self {
        Frame {
            extents: (0, 0, 0, 0),
            visible_name: None,
            name: None,
            wm_name: None,
            window_id: window,
        }
    }

    /// Handle an X11 event for the frame or its decorated window.
    ///
    /// # Arguments
    /// * `window` - X11 window that received the event
    /// * `event_type` - Type of X11 event
    /// * `event_data` - Event data (varies by type)
    ///
    /// # TODO
    /// Port logic from meta_frame_handle_xevent:
    /// - Parse X11 property changes (_NET_WM_NAME, _MOTIF_WM_HINTS, etc.)
    /// - Update frame decorations based on window properties
    /// - Handle window state changes
    /// - Trigger redraw if needed
    pub fn handle_xevent(
        &mut self,
        window: u32,
        event_type: i32,
        event_data: &[u8],
    ) {
        // TODO: port meta_frame_handle_xevent from meta-frame.c
        let _ = (window, event_type, event_data);
    }

    /// Get the frame content widget.
    pub fn content(&self) -> Option<&FrameContent> {
        // TODO: return the frame content child widget
        None
    }

    /// Get the frame header widget.
    pub fn header(&self) -> Option<&FrameHeader> {
        // TODO: return the frame header child widget
        None
    }

    /// Check if frame decorations should be visible.
    pub fn should_show_decorations(&self) -> bool {
        // TODO: port logic from meta_frame_should_show_decorations
        true
    }

    /// Get the undecorated inner size of the window.
    pub fn get_client_size(&self) -> (i32, i32) {
        // TODO: port logic to calculate inner window size minus decorations
        (0, 0)
    }
}
