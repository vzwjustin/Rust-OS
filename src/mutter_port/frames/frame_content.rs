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
}

impl FrameContent {
    /// Create a new frame content widget for an X11 window.
    ///
    /// # Arguments
    /// * `window` - X11 window ID to embed
    ///
    /// # TODO
    /// Port logic from meta_frame_content_new:
    /// - Create GTK widget
    /// - Set up window embedding
    /// - Connect to window events
    pub fn new(window: u32) -> Self {
        FrameContent { window_id: window }
    }

    /// Get the X11 window ID embedded in this content widget.
    pub fn get_window(&self) -> u32 {
        self.window_id
    }
}
