//! Wayland Cursor Surface module
//!
//! Ported from: meta-wayland-cursor-surface.c/h

use alloc::{format, string::String, vec::Vec};

/// Cursor sprite representation: the underlying cursor image buffer
/// plus its associated renderer-private data.
#[derive(Debug)]
pub struct CursorSprite {
    /// Opaque pointer to the MetaCursorSprite (image buffer).
    pub sprite: *mut core::ffi::c_void,
    /// Renderer-private data attached to this sprite (e.g. GPU texture).
    pub renderer_private: *mut core::ffi::c_void,
}

impl CursorSprite {
    pub fn new() -> Self {
        CursorSprite {
            sprite: core::ptr::null_mut(),
            renderer_private: core::ptr::null_mut(),
        }
    }

    pub fn get_sprite(&self) -> *mut core::ffi::c_void {
        self.sprite
    }

    pub fn set_sprite(&mut self, sprite: *mut core::ffi::c_void) {
        self.sprite = sprite;
    }

    pub fn get_renderer_private(&self) -> *mut core::ffi::c_void {
        self.renderer_private
    }

    pub fn set_renderer_private(&mut self, private: *mut core::ffi::c_void) {
        self.renderer_private = private;
    }

    pub fn is_valid(&self) -> bool {
        !self.sprite.is_null()
    }
}

impl Default for CursorSprite {
    fn default() -> Self {
        Self::new()
    }
}

pub struct MetaWaylandCursorSurface {
    pub cursor: Option<*mut core::ffi::c_void>, // ClutterCursor pointer
    pub hotspot_x: i32,
    pub hotspot_y: i32,
    pub renderer: Option<*mut core::ffi::c_void>, // MetaCursorRenderer pointer
    /// Cursor sprite holding the image buffer and renderer-private data.
    pub cursor_sprite: CursorSprite,
}

impl MetaWaylandCursorSurface {
    pub fn new() -> Self {
        MetaWaylandCursorSurface {
            cursor: None,
            hotspot_x: 0,
            hotspot_y: 0,
            renderer: None,
            cursor_sprite: CursorSprite::new(),
        }
    }

    /// Get the cursor for this cursor surface.
    /// Mirrors meta_wayland_cursor_surface_get_cursor.
    pub fn get_cursor(&self) -> Option<*mut core::ffi::c_void> {
        self.cursor
    }

    /// Set the cursor pointer for this surface.
    pub fn set_cursor(&mut self, cursor: Option<*mut core::ffi::c_void>) {
        self.cursor = cursor;
    }

    /// Get the hotspot X coordinate.
    pub fn get_hotspot_x(&self) -> i32 {
        self.hotspot_x
    }

    /// Get the hotspot Y coordinate.
    pub fn get_hotspot_y(&self) -> i32 {
        self.hotspot_y
    }

    /// Set the hotspot coordinates for the cursor.
    /// Mirrors meta_wayland_cursor_surface_set_hotspot.
    pub fn set_hotspot(&mut self, hotspot_x: i32, hotspot_y: i32) {
        self.hotspot_x = hotspot_x;
        self.hotspot_y = hotspot_y;
    }

    /// Get the cursor renderer for this surface.
    pub fn get_renderer(&self) -> Option<*mut core::ffi::c_void> {
        self.renderer
    }

    /// Set the cursor renderer for this surface.
    /// Mirrors meta_wayland_cursor_surface_set_renderer.
    pub fn set_renderer(&mut self, renderer: Option<*mut core::ffi::c_void>) {
        self.renderer = renderer;
    }

    /// Get a reference to the cursor sprite.
    pub fn get_cursor_sprite(&self) -> &CursorSprite {
        &self.cursor_sprite
    }

    /// Get a mutable reference to the cursor sprite.
    pub fn get_cursor_sprite_mut(&mut self) -> &mut CursorSprite {
        &mut self.cursor_sprite
    }

    /// Set the cursor sprite image buffer pointer.
    pub fn set_cursor_sprite_buffer(&mut self, sprite: *mut core::ffi::c_void) {
        self.cursor_sprite.set_sprite(sprite);
    }

    /// Set the renderer-private data on the cursor sprite.
    pub fn set_cursor_sprite_renderer_private(&mut self, private: *mut core::ffi::c_void) {
        self.cursor_sprite.set_renderer_private(private);
    }
}

impl Default for MetaWaylandCursorSurface {
    fn default() -> Self {
        Self::new()
    }
}
