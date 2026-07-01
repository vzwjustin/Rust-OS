//! Window tracker for managing frame decorations across multiple windows.
//! Ported from src/frames/meta-window-tracker.h/c

use alloc::collections::BTreeMap;
use super::Frame;
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
    /// # TODO
    /// Port logic from meta_window_tracker_new:
    /// - Store display reference
    /// - Initialize hash tables for frames and client windows
    /// - Query XInput2 opcode
    /// - Connect to X11 events (CreateNotify, DestroyNotify, etc.)
    /// - Connect to GSettings changes for interface settings
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
    /// # TODO
    /// Port logic to create frame, check WM hints, set up decorations
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

    /// Handle an X11 event.
    ///
    /// # Arguments
    /// * `window_id` - X11 window that received the event
    /// * `event_type` - Type of X11 event
    /// * `event_data` - Event data
    ///
    /// # TODO
    /// Port event handling logic:
    /// - CreateNotify: add new frame
    /// - DestroyNotify: remove frame
    /// - PropertyNotify: update frame properties
    /// - ConfigureNotify: resize decorations
    /// - FocusIn/FocusOut: update focus state
    pub fn handle_xevent(&mut self, window_id: u32, event_type: i32, event_data: &[u8]) {
        // TODO: port meta_window_tracker_handle_xevent
        if let Some(frame) = self.get_frame_mut(window_id) {
            frame.handle_xevent(window_id, event_type, event_data);
        }
    }

    /// Trigger settings change handling (e.g., theme, color scheme).
    ///
    /// # TODO
    /// Port logic from meta_window_tracker_settings_changed
    pub fn on_settings_changed(&mut self) {
        // TODO: iterate frames and update decorations based on new settings
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

// Allow this type to be used with collections
unsafe impl Send for WindowTracker {}
unsafe impl Sync for WindowTracker {}
