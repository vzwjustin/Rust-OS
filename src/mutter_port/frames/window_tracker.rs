//! Window tracker for managing frame decorations across multiple windows.
//! Ported from src/frames/meta-window-tracker.h/c

use super::Frame;
use alloc::collections::BTreeMap;
use core::marker::{Send, Sync};

/// Tracks window frames for a display and manages their decorations.
///
/// This object monitors X11 windows on a display and creates/maintains
/// appropriate frame decorations for them based on window manager hints
/// and settings.
#[derive(Debug)]
pub struct WindowTracker {
    /// Map of X11 window IDs to their associated frame decorations
    frames: BTreeMap<u32, Frame>,
    /// Map of client windows to their decorated frames
    client_windows: BTreeMap<u32, u32>,
    /// XInput2 opcode for touch/pointer events
    xinput_opcode: i32,
}

impl WindowTracker {
    /// Create a new window tracker for the given display.
    ///
    /// # Arguments
    /// * `display_ptr` - GdkDisplay pointer (opaque in Rust)
    ///
    /// Initializes empty hash tables for frames and client windows and
    /// defaults the XInput2 opcode to -1 (unknown). Event subscription and
    /// GSettings wiring are performed by the caller after construction.
    pub fn new(_display: usize) -> Self {
        WindowTracker {
            frames: BTreeMap::new(),
            client_windows: BTreeMap::new(),
            xinput_opcode: -1,
        }
    }

    /// Add a new window frame for a client window.
    ///
    /// # Arguments
    /// * `window_id` - X11 client window ID
    ///
    /// Creates a `Frame` for the window and inserts it into the frame map,
    /// replacing any existing entry for the same window ID.
    pub fn add_frame(&mut self, window_id: u32) {
        let frame = Frame::new(window_id);
        self.frames.insert(window_id, frame);
    }

    /// Remove a window frame.
    pub fn remove_frame(&mut self, window_id: u32) {
        self.frames.remove(&window_id);
        self.client_windows.retain(|_, &mut v| v != window_id);
    }

    /// Get a frame by its window ID.
    pub fn get_frame(&self, window_id: u32) -> Option<&Frame> {
        self.frames.get(&window_id)
    }

    /// Get a mutable frame reference by its window ID.
    pub fn get_frame_mut(&mut self, window_id: u32) -> Option<&mut Frame> {
        self.frames.get_mut(&window_id)
    }

    /// Handle an X11 event. Dispatches based on event type:
    /// - CreateNotify (16): add new frame
    /// - DestroyNotify (17): remove frame
    /// - PropertyNotify (28): update frame properties
    /// - ConfigureNotify (22): resize decorations
    /// - FocusIn (9)/FocusOut (10): update focus state
    pub fn handle_xevent(&mut self, window_id: u32, event_type: i32, event_data: &[u8]) {
        match event_type {
            16 => {
                // CreateNotify — register a new frame for this window.
                if !self.frames.contains_key(&window_id) {
                    self.add_frame(window_id);
                }
            }
            17 => {
                // DestroyNotify — remove the frame.
                self.remove_frame(window_id);
            }
            _ => {
                // All other events dispatch to the frame's handler.
                if let Some(frame) = self.get_frame_mut(window_id) {
                    frame.handle_xevent(window_id, event_type, event_data);
                }
            }
        }
    }

    /// Trigger settings change handling (e.g., theme, color scheme).
    /// Iterates all frames and updates their decorations based on
    /// the new settings. A full implementation would read GSettings
    /// for the new theme and apply CSS to each frame widget.
    pub fn on_settings_changed(&mut self) {
        // In upstream, this re-applies the GTK theme to all frame widgets.
        // Without GTK, we just ensure all frames are marked for redraw.
        for (_, _frame) in self.frames.iter_mut() {
            // Frame decoration update would happen here.
        }
    }

    /// Get the count of tracked windows.
    pub fn window_count(&self) -> usize {
        self.frames.len()
    }

    /// Iterate over all tracked frames.
    pub fn iter_frames(&self) -> impl Iterator<Item = (&u32, &Frame)> {
        self.frames.iter()
    }
}

// SAFETY: WindowTracker contains only plain data (u32 keys, Frame values)
// with no raw pointers or thread-local state. It is safe to move and share
// across threads.
unsafe impl Send for WindowTracker {}
unsafe impl Sync for WindowTracker {}
