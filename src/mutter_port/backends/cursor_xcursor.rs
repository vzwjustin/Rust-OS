//! Cursor Xcursor ported from GNOME Mutter's src/backends/
//!
//! Xcursor theme loading and sprite rendering. Handles cursor shape
//! animations, scaling per output, and xcursor protocol integration.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-cursor-xcursor.c

use alloc::vec::Vec;
use core::ffi::c_void;

/// Cursor type enumeration (from Clutter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ClutterCursorType {
    CLUTTER_CURSOR_DEFAULT = 0,
    CLUTTER_CURSOR_POINTER = 1,
    CLUTTER_CURSOR_MOVE = 2,
    CLUTTER_CURSOR_RESIZE_UP = 3,
    CLUTTER_CURSOR_RESIZE_DOWN = 4,
    CLUTTER_CURSOR_RESIZE_LEFT = 5,
    CLUTTER_CURSOR_RESIZE_RIGHT = 6,
    CLUTTER_CURSOR_RESIZE_UP_LEFT = 7,
    CLUTTER_CURSOR_RESIZE_UP_RIGHT = 8,
    CLUTTER_CURSOR_RESIZE_DOWN_LEFT = 9,
    CLUTTER_CURSOR_RESIZE_DOWN_RIGHT = 10,
    CLUTTER_CURSOR_TEXT = 11,
    CLUTTER_CURSOR_WAIT = 12,
    CLUTTER_CURSOR_NOT_ALLOWED = 13,
    CLUTTER_CURSOR_GRAB = 14,
    CLUTTER_CURSOR_GRABBING = 15,
}

/// Metadata for a single cursor image frame at a specific scale.
#[derive(Debug, Clone)]
pub struct MetaCursorImageData {
    pub scale: i32,
    pub xcursor_images: *mut c_void,
}

/// XCursor-based cursor sprite with theme and animation support.
pub struct MetaCursorXcursor {
    /// Cursor type identifier.
    pub cursor: ClutterCursorType,
    /// Texture for rendering (opaque CoglTexture pointer).
    pub texture: *mut c_void,
    /// Hotspot X coordinate.
    pub hot_x: i32,
    /// Hotspot Y coordinate.
    pub hot_y: i32,
    /// Array of cursor image data at different scales.
    pub cursor_images: Vec<MetaCursorImageData>,
    /// Current animation frame index.
    pub current_frame: i32,
    /// XcursorImages pointer (opaque).
    pub xcursor_images: *mut c_void,
    /// Theme scale factor.
    pub theme_scale: i32,
    /// Flag indicating if texture needs reloading.
    pub invalidated: bool,
}

impl MetaCursorXcursor {
    /// Create a new cursor sprite.
    pub fn new(cursor_type: ClutterCursorType) -> Self {
        MetaCursorXcursor {
            cursor: cursor_type,
            texture: core::ptr::null_mut(),
            hot_x: 0,
            hot_y: 0,
            cursor_images: Vec::new(),
            current_frame: 0,
            xcursor_images: core::ptr::null_mut(),
            theme_scale: 1,
            invalidated: false,
        }
    }

    /// Get an xcursor sprite by type.
    pub fn get(_cursor_type: ClutterCursorType) -> Option<Self> {
        // TODO: Load cursor from xcursor theme
        None
    }

    /// Set the theme scale factor.
    pub fn set_theme_scale(&mut self, scale: i32) {
        self.theme_scale = scale;
        self.invalidated = true;
    }

    /// Get the cursor type.
    pub fn get_cursor(&self) -> ClutterCursorType {
        self.cursor
    }

    /// Get the current image frame.
    pub fn get_current_image(&self) -> Option<*mut c_void> {
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
        MetaCursorXcursor::new(ClutterCursorType::CLUTTER_CURSOR_DEFAULT)
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
