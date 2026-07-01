//! Cursor Xcursor ported from GNOME Mutter's src/backends/
//!
//! Xcursor theme loading and sprite rendering. Handles cursor shape
//! animations, scaling per output, and xcursor protocol integration.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-cursor-xcursor.c

/// Cursor type enumeration (from Clutter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ClutterCursorType {
    // TODO: Define standard cursor types (ARROW, TEXT, HAND, etc.)
    Arrow = 0,
}

/// XCursor-based cursor sprite with theme and animation support.
pub struct MetaCursorXcursor {
    // TODO: cursor_type, xcursor_image, theme_scale, animation fields
}

impl MetaCursorXcursor {
    /// Get an xcursor sprite by type.
    pub fn get(_cursor_type: ClutterCursorType) -> Option<Self> {
        // TODO: Load cursor from xcursor theme
        None
    }

    /// Set the theme scale factor.
    pub fn set_theme_scale(&mut self, _scale: i32) {
        // TODO: Reload or resample cursor at new scale
    }

    /// Get the cursor type.
    pub fn get_cursor(&self) -> ClutterCursorType {
        // TODO: Return cursor type
        ClutterCursorType::Arrow
    }

    /// Get the current image frame.
    pub fn get_current_image(&self) -> Option<()> {
        // TODO: Return XcursorImage with animation frame
        None
    }

    /// Get scaled image dimensions.
    pub fn get_scaled_image_size(&self) -> (i32, i32) {
        // TODO: Return width, height at current theme scale
        (0, 0)
    }
}

impl Default for MetaCursorXcursor {
    fn default() -> Self {
        MetaCursorXcursor {}
    }
}

/// Get standardized cursor name from cursor type.
pub fn meta_cursor_get_name(_cursor: ClutterCursorType) -> Option<&'static str> {
    // TODO: Map cursor type to CSS/XCursor name
    None
}

/// Get legacy X11 cursor name.
pub fn meta_cursor_get_legacy_name(_cursor: ClutterCursorType) -> Option<&'static str> {
    // TODO: Map cursor type to legacy X cursor name
    None
}
