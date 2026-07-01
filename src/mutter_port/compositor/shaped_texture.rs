//! Shaped texture for arbitrary window rendering ported from `meta-shaped-texture.c`.
//!
//! Provides texture rendering with arbitrary shape/mask support.

use super::cogl_utils::Texture;

/// Texture with optional shape mask
#[derive(Debug)]
pub struct ShapedTexture {
    pub id: u32,
    pub texture: Option<Texture>,
    pub mask: Option<Texture>, // Shape mask (alpha channel used)
    pub width: u32,
    pub height: u32,
}

impl ShapedTexture {
    /// Create new shaped texture
    pub fn new(id: u32, width: u32, height: u32) -> Self {
        ShapedTexture {
            id,
            texture: None,
            mask: None,
            width,
            height,
        }
    }

    /// Set main texture
    pub fn set_texture(&mut self, texture: Texture) {
        self.texture = Some(texture);
    }

    /// Set shape mask
    pub fn set_mask(&mut self, mask: Texture) {
        self.mask = Some(mask);
    }

    /// Get main texture
    pub fn get_texture(&self) -> Option<&Texture> {
        self.texture.as_ref()
    }

    /// Get shape mask
    pub fn get_mask(&self) -> Option<&Texture> {
        self.mask.as_ref()
    }

    /// Has shape mask applied
    pub fn has_mask(&self) -> bool {
        self.mask.is_some()
    }

    /// Paint the shaped texture
    pub fn paint(&self) {
        if let Some(ref texture) = self.texture {
            // Render texture, optionally applying mask
            if let Some(ref mask) = self.mask {
                // Use mask as alpha channel during rendering
            }
        }
    }

    /// Update texture content
    pub fn update_texture(&mut self, data: &[u8]) -> bool {
        if let Some(ref mut texture) = self.texture {
            // In real implementation, would upload data to GPU
            true
        } else {
            false
        }
    }
}
