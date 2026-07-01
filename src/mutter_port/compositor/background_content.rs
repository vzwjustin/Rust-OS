//! Background content rendering ported from `meta-background-content.c`.
//!
//! Manages GPU texture content for background rendering with efficient updates.

use super::cogl_utils::{Texture, TextureComponents};
use alloc::vec::Vec;

/// Content source type for background rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    /// Solid color
    Color,
    /// Image texture
    Image,
    /// Video stream
    Video,
}

/// Background content object managing GPU textures
#[derive(Debug)]
pub struct BackgroundContent {
    pub id: u32,
    pub texture: Option<Texture>,
    pub content_type: ContentType,
    pub width: u32,
    pub height: u32,
    pub dirty: bool,
}

impl BackgroundContent {
    /// Create new background content
    pub fn new(id: u32, width: u32, height: u32) -> Self {
        BackgroundContent {
            id,
            texture: None,
            content_type: ContentType::Color,
            width,
            height,
            dirty: true,
        }
    }

    /// Set texture content
    pub fn set_texture(&mut self, texture: Texture) {
        self.texture = Some(texture);
        self.content_type = ContentType::Image;
        self.dirty = true;
    }

    /// Mark content as needing update
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Update GPU texture from source data
    pub fn update(&mut self, data: &[u8]) -> bool {
        if let Some(ref mut texture) = self.texture {
            // In real implementation, would upload to GPU
            self.dirty = false;
            true
        } else {
            false
        }
    }

    /// Get texture for rendering
    pub fn get_texture(&self) -> Option<&Texture> {
        self.texture.as_ref()
    }

    /// Is content dirty (needs GPU update)
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Get memory size of texture content
    pub fn memory_size(&self) -> usize {
        self.texture.as_ref().map(|t| t.memory_size()).unwrap_or(0)
    }
}
