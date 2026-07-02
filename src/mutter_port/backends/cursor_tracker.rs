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

    /// Set the pointer position.
    pub fn set_pointer_position(&mut self, x: f32, y: f32) {
        self.pointer_x = x;
        self.pointer_y = y;
    }

    /// Set the cursor scale factor.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    /// Get cursor hotspot offset. Returns (0, 0) when no cursor sprite
    /// is loaded; a full implementation would read the hotspot from the
    /// current cursor sprite.
    pub fn get_hot(&self) -> (i32, i32) {
        if self.current_cursor.is_null() {
            (0, 0)
        } else {
            // Hotspot would be read from the cursor sprite metadata.
            (0, 0)
        }
    }

    /// Get current cursor sprite texture. Returns None when no cursor
    /// is loaded.
    pub fn get_sprite(&self) -> Option<()> {
        if self.current_cursor.is_null() {
            None
        } else {
            Some(())
        }
    }

    /// Get cursor scale factor.
    pub fn get_scale(&self) -> f32 {
        self.scale
    }

    /// Get pointer coordinates.
    pub fn get_pointer(&self) -> (f64, f64) {
        (self.pointer_x as f64, self.pointer_y as f64)
    }

    /// Get pointer visibility state. The cursor is visible when there
    /// are no visibility inhibitors.
    pub fn get_pointer_visible(&self) -> bool {
        self.cursor_visibility_inhibitors == 0
    }

    /// Inhibit cursor visibility. Increments the inhibitor count.
    pub fn inhibit_cursor_visibility(&mut self) {
        self.cursor_visibility_inhibitors += 1;
    }

    /// Restore cursor visibility. Decrements the inhibitor count,
    /// saturating at zero.
    pub fn uninhibit_cursor_visibility(&mut self) {
        if self.cursor_visibility_inhibitors > 0 {
            self.cursor_visibility_inhibitors -= 1;
        }
    }
}

impl Default for MetaCursorTracker {
    fn default() -> Self {
        Self::new()
    }
}
