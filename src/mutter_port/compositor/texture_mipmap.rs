//! Mipmapped texture support ported from `meta-texture-mipmap.c`.
//!
//! Provides mipmap generation and filtering for improved texture rendering.

use super::cogl_utils::Texture;
use alloc::vec::Vec;

/// Mipmap level
#[derive(Debug, Clone)]
pub struct MipmapLevel {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub texture: Option<Texture>,
}

impl MipmapLevel {
    /// Create new mipmap level
    pub fn new(level: u32, width: u32, height: u32) -> Self {
        MipmapLevel {
            level,
            width,
            height,
            texture: None,
        }
    }
}

/// Mipmapped texture chain
#[derive(Debug)]
pub struct TextureMipmap {
    pub id: u32,
    pub base_texture: Texture,
    pub levels: Vec<MipmapLevel>,
    pub auto_generate: bool,
}

impl TextureMipmap {
    /// Create new mipmapped texture
    pub fn new(id: u32, base: Texture) -> Self {
        let mut levels = Vec::new();
        levels.push(MipmapLevel::new(0, base.width, base.height));

        TextureMipmap {
            id,
            base_texture: base,
            levels,
            auto_generate: true,
        }
    }

    /// Generate mipmaps from base texture
    pub fn generate_mipmaps(&mut self) {
        let mut level = 0;
        let mut width = self.base_texture.width;
        let mut height = self.base_texture.height;

        while width > 1 || height > 1 {
            width = width.max(1) / 2;
            height = height.max(1) / 2;
            level += 1;

            self.levels.push(MipmapLevel::new(level, width, height));
        }
    }

    /// Get mipmap level by index
    pub fn get_level(&self, level: u32) -> Option<&MipmapLevel> {
        self.levels.iter().find(|l| l.level == level)
    }

    /// Get mutable mipmap level
    pub fn get_level_mut(&mut self, level: u32) -> Option<&mut MipmapLevel> {
        self.levels.iter_mut().find(|l| l.level == level)
    }

    /// Get level count
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }

    /// Enable/disable auto-generation
    pub fn set_auto_generate(&mut self, enabled: bool) {
        self.auto_generate = enabled;
    }

    /// Total memory used by all levels
    pub fn total_memory(&self) -> usize {
        self.levels
            .iter()
            .filter_map(|l| l.texture.as_ref().map(|t| t.memory_size()))
            .sum()
    }
}
