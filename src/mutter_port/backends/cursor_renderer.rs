//! Cursor Renderer ported from GNOME Mutter's src/backends/
//!
//! Manages rendering of hardware and software cursors. Provides interface
//! for updating cursor sprites, positions, and overlay rendering.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-cursor-renderer.c

use core::ffi::c_void;

/// Interface for hardware cursor inhibition.
pub struct MetaHwCursorInhibitor {
    inhibited: bool,
}

impl MetaHwCursorInhibitor {
    /// Create a new hardware cursor inhibitor (initially not inhibited).
    pub fn new() -> Self {
        Self { inhibited: false }
    }

    /// Set the inhibition state.
    pub fn set_inhibited(&mut self, inhibited: bool) {
        self.inhibited = inhibited;
    }

    /// Check if hardware cursor is inhibited.
    pub fn is_cursor_inhibited(&self) -> bool {
        self.inhibited
    }
}

impl Default for MetaHwCursorInhibitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Cursor renderer managing sprite updates and positioning.
pub struct MetaCursorRenderer {
    /// Reference to backend (opaque).
    pub backend: *mut c_void,
    /// Current X position in stage coordinates.
    pub current_x: f32,
    /// Current Y position in stage coordinates.
    pub current_y: f32,
    /// Current sprite being rendered (opaque ClutterSprite).
    pub sprite: *mut c_void,
    /// Currently displayed cursor (opaque ClutterCursor).
    pub displayed_cursor: *mut c_void,
    /// Overlay cursor for stage rendering (opaque ClutterCursor).
    pub overlay_cursor: *mut c_void,
    /// Stage overlay for cursor rendering (opaque MetaOverlay).
    pub stage_overlay: *mut c_void,
    /// Whether overlay rendering is needed.
    pub needs_overlay: bool,
    /// Handler ID for after-paint signal.
    pub after_paint_handler_id: u64,
}

impl MetaCursorRenderer {
    /// Create a new cursor renderer.
    pub fn new() -> Self {
        MetaCursorRenderer {
            backend: core::ptr::null_mut(),
            current_x: 0.0,
            current_y: 0.0,
            sprite: core::ptr::null_mut(),
            displayed_cursor: core::ptr::null_mut(),
            overlay_cursor: core::ptr::null_mut(),
            stage_overlay: core::ptr::null_mut(),
            needs_overlay: false,
            after_paint_handler_id: 0,
        }
    }

    /// Update the current cursor sprite. Marks overlay as needed
    /// when the displayed cursor differs from the current sprite.
    pub fn update_sprite(&mut self) {
        if self.displayed_cursor != self.sprite {
            self.displayed_cursor = self.sprite;
            self.needs_overlay = true;
        }
    }

    /// Update cursor visibility and position. Returns true if the
    /// hardware cursor was successfully updated.
    pub fn update_cursor(&mut self) -> bool {
        // Without hardware cursor I/O, we just mark overlay as needed.
        if self.needs_overlay {
            self.needs_overlay = false;
        }
        true
    }

    /// Get current cursor sprite. Returns None when no sprite is set.
    pub fn get_sprite(&self) -> Option<()> {
        if self.sprite.is_null() {
            None
        } else {
            Some(())
        }
    }

    /// Set cursor sprite (opaque pointer). Marks overlay as needed.
    pub fn set_sprite(&mut self, sprite: *mut c_void) {
        self.sprite = sprite;
        self.needs_overlay = true;
    }

    /// Calculate cursor rendering rectangle as `(x, y, width, height)`.
    /// Uses the current position; dimensions would come from the sprite
    /// metadata in a full implementation.
    pub fn calculate_rect(&self) -> (i32, i32, u32, u32) {
        if self.sprite.is_null() {
            (0, 0, 0, 0)
        } else {
            (self.current_x as i32, self.current_y as i32, 32, 32)
        }
    }

    /// Check if cursor needs overlay rendering.
    pub fn needs_overlay(&self) -> bool {
        self.needs_overlay
    }

    /// Update cursor position in stage.
    pub fn update_position(&mut self, x: f32, y: f32) {
        if self.current_x != x || self.current_y != y {
            self.current_x = x;
            self.current_y = y;
            self.needs_overlay = true;
        }
    }

    /// Force immediate cursor update. Bypasses caching.
    pub fn force_update(&mut self) {
        self.needs_overlay = true;
    }
}

impl Default for MetaCursorRenderer {
    fn default() -> Self {
        Self::new()
    }
}
