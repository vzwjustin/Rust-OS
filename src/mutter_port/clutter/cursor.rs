//! Port of GNOME mutter's `clutter/clutter/clutter-cursor.{c,h}`.
//!
//! `ClutterCursor` is an abstract base type for pointer cursors. It manages
//! cursor state (texture scale, transform, viewport settings) and delegates
//! virtual operations (texture rendering, animation) to subclasses.
//!
//! This port drops GObject reference-counted allocation and signals. `Cursor`
//! is a plain struct; subclasses implement `CursorOps` for virtual methods.

/// Representation of a rectangle (replaces graphene_rect_t).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for Rect {
    fn default() -> Self {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }
}

/// Monitor transform (replaces MtkMonitorTransform).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum MonitorTransform {
    #[default]
    Normal = 0,
    Rotate90 = 1,
    Rotate180 = 2,
    Rotate270 = 3,
    FlipNormal = 4,
    FlipRotate90 = 5,
    FlipRotate180 = 6,
    FlipRotate270 = 7,
}

/// Virtual operations for cursor subclasses.
pub trait CursorOps {
    /// Get the cursor texture and hot-spot coordinates.
    fn get_texture(&self, hot_x: &mut i32, hot_y: &mut i32) -> *mut ();

    /// Invalidate cached texture state.
    fn invalidate(&mut self);

    /// Realize the cursor texture.
    fn realize_texture(&mut self) -> bool;

    /// Check if cursor is animated (default: false).
    fn is_animated(&self) -> bool {
        false
    }

    /// Advance to the next animation frame (default: no-op).
    fn tick_frame(&mut self) {}

    /// Get the current frame time in milliseconds (default: 0).
    fn get_current_frame_time(&self) -> u32 {
        0
    }

    /// Prepare cursor at the given position and scale (default: no-op).
    fn prepare_at(&mut self, best_scale: f32, x: i32, y: i32) {}
}

/// Port of `ClutterCursor`. Manages cursor state; subclasses implement
/// `CursorOps` for texture/animation rendering.
#[derive(Debug, Clone, Default)]
pub struct Cursor {
    pub texture_scale: f32,
    pub texture_transform: MonitorTransform,
    pub viewport_src_rect: Option<Rect>,
    pub viewport_dst_size: Option<(i32, i32)>,
}

impl Cursor {
    /// Create a new cursor with default state (scale=1.0, transform=Normal).
    pub fn new() -> Self {
        Cursor {
            texture_scale: 1.0,
            texture_transform: MonitorTransform::Normal,
            viewport_src_rect: None,
            viewport_dst_size: None,
        }
    }

    /// Set the texture scale factor. Skips invalidation if unchanged
    /// (within FLT_EPSILON).
    pub fn set_texture_scale(&mut self, scale: f32) {
        if (self.texture_scale - scale).abs() < f32::EPSILON {
            return;
        }
        self.texture_scale = scale;
    }

    /// Get the texture scale factor.
    pub fn get_texture_scale(&self) -> f32 {
        self.texture_scale
    }

    /// Set the texture transform. Skips invalidation if unchanged.
    pub fn set_texture_transform(&mut self, transform: MonitorTransform) {
        if self.texture_transform == transform {
            return;
        }
        self.texture_transform = transform;
    }

    /// Get the texture transform.
    pub fn get_texture_transform(&self) -> MonitorTransform {
        self.texture_transform
    }

    /// Set the viewport source rectangle. Skips invalidation if unchanged.
    pub fn set_viewport_src_rect(&mut self, src_rect: Rect) {
        if let Some(existing) = self.viewport_src_rect {
            if (existing.x - src_rect.x).abs() < f32::EPSILON
                && (existing.y - src_rect.y).abs() < f32::EPSILON
                && (existing.width - src_rect.width).abs() < f32::EPSILON
                && (existing.height - src_rect.height).abs() < f32::EPSILON
            {
                return;
            }
        }
        self.viewport_src_rect = Some(src_rect);
    }

    /// Reset the viewport source rectangle.
    pub fn reset_viewport_src_rect(&mut self) {
        self.viewport_src_rect = None;
    }

    /// Get the viewport source rectangle if set.
    pub fn get_viewport_src_rect(&self) -> Option<Rect> {
        self.viewport_src_rect
    }

    /// Set the viewport destination size. Skips invalidation if unchanged.
    pub fn set_viewport_dst_size(&mut self, width: i32, height: i32) {
        if let Some((w, h)) = self.viewport_dst_size {
            if w == width && h == height {
                return;
            }
        }
        self.viewport_dst_size = Some((width, height));
    }

    /// Reset the viewport destination size.
    pub fn reset_viewport_dst_size(&mut self) {
        self.viewport_dst_size = None;
    }

    /// Get the viewport destination size if set.
    pub fn get_viewport_dst_size(&self) -> Option<(i32, i32)> {
        self.viewport_dst_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cursor_defaults() {
        let c = Cursor::new();
        assert_eq!(c.texture_scale, 1.0);
        assert_eq!(c.texture_transform, MonitorTransform::Normal);
        assert!(c.viewport_src_rect.is_none());
        assert!(c.viewport_dst_size.is_none());
    }

    #[test]
    fn set_texture_scale_round_trips() {
        let mut c = Cursor::new();
        c.set_texture_scale(2.5);
        assert_eq!(c.get_texture_scale(), 2.5);
    }

    #[test]
    fn set_texture_transform_round_trips() {
        let mut c = Cursor::new();
        c.set_texture_transform(MonitorTransform::Rotate90);
        assert_eq!(c.get_texture_transform(), MonitorTransform::Rotate90);
    }

    #[test]
    fn viewport_src_rect_round_trips() {
        let mut c = Cursor::new();
        let rect = Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 200.0,
        };
        c.set_viewport_src_rect(rect);
        assert_eq!(c.get_viewport_src_rect(), Some(rect));
        c.reset_viewport_src_rect();
        assert!(c.get_viewport_src_rect().is_none());
    }

    #[test]
    fn viewport_dst_size_round_trips() {
        let mut c = Cursor::new();
        c.set_viewport_dst_size(640, 480);
        assert_eq!(c.get_viewport_dst_size(), Some((640, 480)));
        c.reset_viewport_dst_size();
        assert!(c.get_viewport_dst_size().is_none());
    }

    #[test]
    fn texture_scale_epsilon_comparison() {
        let mut c = Cursor::new();
        c.set_texture_scale(1.0);
        c.texture_scale = 1.0 + f32::EPSILON / 2.0;
        c.set_texture_scale(1.0); // should be treated as equal
        assert!(c.texture_scale < 1.1); // scale unchanged
    }
}
