//! Cursor Tracker ported from GNOME Mutter's src/backends/
//!
//! Tracks cursor position, theme, sprite visibility, and generates
//! motion/update events for window managers and clients.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-cursor-tracker.c

use core::ffi::c_void;

/// Tracks pointer position, cursor sprite, and visibility state.
pub struct MetaCursorTracker {
    /// Pointer to MetaBackend instance (opaque).
    pub backend: *mut c_void,
    /// Current cursor object (opaque ClutterCursor pointer).
    pub current_cursor: *mut c_void,
    /// Cursor visibility inhibition count.
    pub cursor_visibility_inhibitors: i32,
    /// Pointer X coordinate.
    pub pointer_x: f32,
    /// Pointer Y coordinate.
    pub pointer_y: f32,
    /// Cursor scale factor for HiDPI.
    pub scale: f32,
}

impl MetaCursorTracker {
    /// Create a new cursor tracker.
    pub fn new() -> Self {
        MetaCursorTracker {
            backend: core::ptr::null_mut(),
            current_cursor: core::ptr::null_mut(),
            cursor_visibility_inhibitors: 0,
            pointer_x: 0.0,
            pointer_y: 0.0,
            scale: 1.0,
        }
    }

    /// Get cursor hotspot offset.
    pub fn get_hot(&self) -> (i32, i32) {
        // TODO: Return hotspot from current sprite
        (0, 0)
    }

    /// Get current cursor sprite texture.
    pub fn get_sprite(&self) -> Option<()> {
        // TODO: Return CoglTexture when available
        None
    }

    /// Get cursor scale factor.
    pub fn get_scale(&self) -> f32 {
        // TODO: Return scale from backend
        1.0
    }

    /// Get pointer coordinates.
    pub fn get_pointer(&self) -> (f64, f64) {
        // TODO: Return current pointer position
        (0.0, 0.0)
    }

    /// Get pointer visibility state.
    pub fn get_pointer_visible(&self) -> bool {
        // TODO: Check visibility inhibitors
        true
    }

    /// Inhibit cursor visibility.
    pub fn inhibit_cursor_visibility(&mut self) {
        // TODO: Increment visibility inhibitor count
    }

    /// Restore cursor visibility.
    pub fn uninhibit_cursor_visibility(&mut self) {
        // TODO: Decrement visibility inhibitor count
    }
}

impl Default for MetaCursorTracker {
    fn default() -> Self {
        Self::new()
    }
}
