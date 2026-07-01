//! Multi-texture GPU handling ported from `meta-multi-texture.c`.
//!
//! Manages multiple texture layers for advanced rendering effects.

use super::cogl_utils::{Texture, TextureComponents};
use alloc::vec::Vec;

/// Multiple texture layers for rendering
#[derive(Debug)]
pub struct MultiTexture {
    pub id: u32,
    pub textures: Vec<Texture>,
    pub width: u32,
    pub height: u32,
}

impl MultiTexture {
    /// Create new multi-texture with specified layers
    pub fn new(id: u32, width: u32, height: u32) -> Self {
        MultiTexture {
            id,
            textures: Vec::new(),
            width,
            height,
        }
    }

    /// Add texture layer
    pub fn add_layer(&mut self, texture: Texture) {
        self.textures.push(texture);
    }

    /// Get texture layer by index
    pub fn get_layer(&self, index: usize) -> Option<&Texture> {
        self.textures.get(index)
    }

    /// Get mutable texture layer
    pub fn get_layer_mut(&mut self, index: usize) -> Option<&mut Texture> {
        self.textures.get_mut(index)
    }

    /// Get layer count
    pub fn layer_count(&self) -> usize {
        self.textures.len()
    }

    /// Remove texture layer
    pub fn remove_layer(&mut self, index: usize) {
        if index < self.textures.len() {
            self.textures.remove(index);
        }
    }

    /// Total memory used by all layers
    pub fn total_memory(&self) -> usize {
        self.textures.iter().map(|t| t.memory_size()).sum()
    }
}
