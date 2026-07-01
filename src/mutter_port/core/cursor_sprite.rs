//! MetaCursorSprite ported from GNOME Mutter's src/core/meta-cursor-sprite.c
//!
//! MetaCursorSprite represents a single cursor image: the texture data,
//! hotspot, scale, and transform. In Mutter this is a GObject that wraps a
//! Cogl texture and provides scaling/rotation for HiDPI and transformed
//! monitors.
//!
//! In the kernel, Cogl textures are not available. The sprite is modeled as
//! a plain struct with the image metadata (dimensions, hotspot, scale,
//! transform) and an opaque texture handle that the backend can use to
//! reference the actual pixel data.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-cursor-sprite.c

use super::cursor_tracker::CursorSpriteId;
use crate::mutter_port::backends::logical_monitor::MonitorTransform;

/// Cursor image metadata. Mirrors MetaCursorSprite.
#[derive(Debug, Clone)]
pub struct CursorSprite {
    /// Opaque texture handle (CoglTexture in Mutter).
    texture_id: u64,
    /// Width of the cursor image in pixels.
    width: u32,
    /// Height of the cursor image in pixels.
    height: u32,
    /// Hotspot X offset (the click point within the image).
    hotspot_x: i32,
    /// Hotspot Y offset.
    hotspot_y: i32,
    /// Scale factor (for HiDPI cursors).
    scale: f32,
    /// Transform applied to the cursor (for rotated monitors).
    transform: MonitorTransform,
    /// Whether the texture needs to be re-uploaded to the backend.
    texture_dirty: bool,
    /// Whether the scale/transform changed and the cursor needs re-rendering.
    metadata_dirty: bool,
}

impl CursorSprite {
    /// Create a new cursor sprite. Mirrors meta_cursor_sprite_new().
    pub fn new(texture_id: u64, width: u32, height: u32) -> Self {
        CursorSprite {
            texture_id,
            width,
            height,
            hotspot_x: 0,
            hotspot_y: 0,
            scale: 1.0,
            transform: MonitorTransform::Normal,
            texture_dirty: true,
            metadata_dirty: true,
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────

    pub fn texture_id(&self) -> u64 {
        self.texture_id
    }

    pub fn set_texture_id(&mut self, id: u64) {
        if self.texture_id != id {
            self.texture_id = id;
            self.texture_dirty = true;
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn hotspot(&self) -> (i32, i32) {
        (self.hotspot_x, self.hotspot_y)
    }

    pub fn set_hotspot(&mut self, x: i32, y: i32) {
        if self.hotspot_x != x || self.hotspot_y != y {
            self.hotspot_x = x;
            self.hotspot_y = y;
            self.metadata_dirty = true;
        }
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn set_scale(&mut self, scale: f32) {
        if (self.scale - scale).abs() > f32::EPSILON {
            self.scale = scale;
            self.metadata_dirty = true;
        }
    }

    pub fn transform(&self) -> MonitorTransform {
        self.transform
    }

    pub fn set_transform(&mut self, transform: MonitorTransform) {
        if self.transform != transform {
            self.transform = transform;
            self.metadata_dirty = true;
        }
    }

    // ── Dirty tracking ────────────────────────────────────────────────

    pub fn is_texture_dirty(&self) -> bool {
        self.texture_dirty
    }

    pub fn clear_texture_dirty(&mut self) {
        self.texture_dirty = false;
    }

    pub fn is_metadata_dirty(&self) -> bool {
        self.metadata_dirty
    }

    pub fn clear_metadata_dirty(&mut self) {
        self.metadata_dirty = false;
    }

    /// Mark the sprite as fully clean (after backend has uploaded and
    /// rendered it).
    pub fn clear_dirty(&mut self) {
        self.texture_dirty = false;
        self.metadata_dirty = false;
    }

    // ── Computed geometry ─────────────────────────────────────────────

    /// The scaled hotspot position, accounting for transform.
    /// Mirrors the hotspot adjustment in meta_cursor_sprite_get_hotspot().
    pub fn scaled_hotspot(&self) -> (i32, i32) {
        let sx = (self.hotspot_x as f32 * self.scale) as i32;
        let sy = (self.hotspot_y as f32 * self.scale) as i32;
        match self.transform {
            MonitorTransform::Rotate90 | MonitorTransform::FlippedRotate90 => {
                let h = self.height as f32 * self.scale;
                (sy, (h as i32) - sx - 1)
            }
            MonitorTransform::Rotate180 | MonitorTransform::FlippedRotate180 => {
                let w = self.width as f32 * self.scale;
                let h = self.height as f32 * self.scale;
                ((w as i32) - sx - 1, (h as i32) - sy - 1)
            }
            MonitorTransform::Rotate270 | MonitorTransform::FlippedRotate270 => {
                let w = self.width as f32 * self.scale;
                ((w as i32) - sy - 1, sx)
            }
            _ => (sx, sy),
        }
    }

    /// Scaled dimensions, accounting for transform (90°/270° swap w/h).
    pub fn scaled_dimensions(&self) -> (u32, u32) {
        let sw = (self.width as f32 * self.scale) as u32;
        let sh = (self.height as f32 * self.scale) as u32;
        match self.transform {
            MonitorTransform::Rotate90
            | MonitorTransform::Rotate270
            | MonitorTransform::FlippedRotate90
            | MonitorTransform::FlippedRotate270 => (sh, sw),
            _ => (sw, sh),
        }
    }

    /// Get the opaque sprite id for use with the cursor tracker.
    pub fn sprite_id(&self) -> CursorSpriteId {
        self.texture_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creation() {
        let sprite = CursorSprite::new(42, 32, 32);
        assert_eq!(sprite.texture_id(), 42);
        assert_eq!(sprite.width(), 32);
        assert_eq!(sprite.height(), 32);
        assert_eq!(sprite.hotspot(), (0, 0));
        assert_eq!(sprite.scale(), 1.0);
        assert!(sprite.is_texture_dirty());
        assert!(sprite.is_metadata_dirty());
    }

    #[test]
    fn test_hotspot() {
        let mut sprite = CursorSprite::new(1, 24, 24);
        sprite.set_hotspot(12, 8);
        assert_eq!(sprite.hotspot(), (12, 8));
        assert!(sprite.is_metadata_dirty());
        sprite.clear_metadata_dirty();
        assert!(!sprite.is_metadata_dirty());
    }

    #[test]
    fn test_scale() {
        let mut sprite = CursorSprite::new(1, 32, 32);
        sprite.set_scale(2.0);
        assert_eq!(sprite.scale(), 2.0);
        assert_eq!(sprite.scaled_dimensions(), (64, 64));
    }

    #[test]
    fn test_scaled_hotspot() {
        let mut sprite = CursorSprite::new(1, 32, 32);
        sprite.set_hotspot(8, 4);
        sprite.set_scale(2.0);
        // Scale 2.0: hotspot becomes (16, 8)
        assert_eq!(sprite.scaled_hotspot(), (16, 8));
    }

    #[test]
    fn test_rotated_dimensions() {
        let mut sprite = CursorSprite::new(1, 32, 16);
        sprite.set_transform(MonitorTransform::Rotate90);
        // 90° rotation swaps width and height.
        assert_eq!(sprite.scaled_dimensions(), (16, 32));
    }

    #[test]
    fn test_clear_dirty() {
        let mut sprite = CursorSprite::new(1, 32, 32);
        sprite.clear_dirty();
        assert!(!sprite.is_texture_dirty());
        assert!(!sprite.is_metadata_dirty());
    }

    #[test]
    fn test_texture_change_marks_dirty() {
        let mut sprite = CursorSprite::new(1, 32, 32);
        sprite.clear_dirty();
        assert!(!sprite.is_texture_dirty());

        sprite.set_texture_id(2);
        assert!(sprite.is_texture_dirty());
    }
}
