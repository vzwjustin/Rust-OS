//! Surface actor for window content ported from `meta-surface-actor.c`.
//!
//! Base actor for rendering window surface content.

use super::shaped_texture::ShapedTexture;

/// Base surface actor for window content
#[derive(Debug)]
pub struct SurfaceActor {
    pub id: u32,
    pub shaped_texture: Option<ShapedTexture>,
    pub visible: bool,
    pub opacity: f32,
    pub x: i32,
    pub y: i32,
}

impl SurfaceActor {
    /// Create new surface actor
    pub fn new(id: u32) -> Self {
        SurfaceActor {
            id,
            shaped_texture: None,
            visible: true,
            opacity: 1.0,
            x: 0,
            y: 0,
        }
    }

    /// Set shaped texture for this surface
    pub fn set_texture(&mut self, texture: ShapedTexture) {
        self.shaped_texture = Some(texture);
    }

    /// Get texture
    pub fn get_texture(&self) -> Option<&ShapedTexture> {
        self.shaped_texture.as_ref()
    }

    /// Set visibility
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Set opacity
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.max(0.0).min(1.0);
    }

    /// Set position
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    /// Paint surface
    pub fn paint(&self) {
        if !self.visible || self.opacity == 0.0 {
            return;
        }

        if let Some(ref texture) = self.shaped_texture {
            texture.paint();
        }
    }
}
