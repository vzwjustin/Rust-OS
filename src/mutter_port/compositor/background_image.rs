//! Background image handling ported from `meta-background-image.c`.
//!
//! Manages image resources for desktop backgrounds.

use alloc::vec::Vec;

/// Image scaling mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageScale {
    /// Maintain aspect ratio, may leave borders
    AspectRatio,
    /// Stretch to fill completely
    Fill,
    /// Tile across the screen
    Tile,
    /// Center without scaling
    Center,
}

/// Background image resource
#[derive(Debug)]
pub struct BackgroundImage {
    pub id: u32,
    pub path: usize, // String reference
    pub width: u32,
    pub height: u32,
    pub scale: ImageScale,
    pub loaded: bool,
    pub data: Vec<u8>,
}

impl BackgroundImage {
    /// Create new background image
    pub fn new(id: u32, path_ref: usize) -> Self {
        BackgroundImage {
            id,
            path: path_ref,
            width: 0,
            height: 0,
            scale: ImageScale::AspectRatio,
            loaded: false,
            data: Vec::new(),
        }
    }

    /// Load image from file
    pub fn load(&mut self) -> bool {
        // Placeholder for file loading logic
        self.loaded = true;
        true
    }

    /// Set image dimensions
    pub fn set_size(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    /// Set scaling mode
    pub fn set_scale(&mut self, scale: ImageScale) {
        self.scale = scale;
    }

    /// Check if image is loaded
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Get image pixel data
    pub fn get_data(&self) -> &[u8] {
        &self.data
    }
}

/// Image cache manager
#[derive(Debug)]
pub struct ImageCache {
    images: Vec<BackgroundImage>,
}

impl ImageCache {
    /// Create new image cache
    pub fn new() -> Self {
        ImageCache { images: Vec::new() }
    }

    /// Add image to cache
    pub fn add_image(&mut self, image: BackgroundImage) -> u32 {
        let id = image.id;
        self.images.push(image);
        id
    }

    /// Get image from cache
    pub fn get_image(&self, id: u32) -> Option<&BackgroundImage> {
        self.images.iter().find(|img| img.id == id)
    }

    /// Get mutable image from cache
    pub fn get_image_mut(&mut self, id: u32) -> Option<&mut BackgroundImage> {
        self.images.iter_mut().find(|img| img.id == id)
    }

    /// Remove image from cache
    pub fn remove_image(&mut self, id: u32) {
        self.images.retain(|img| img.id != id);
    }

    /// Clear entire cache
    pub fn clear(&mut self) {
        self.images.clear();
    }

    /// Get cache size
    pub fn size(&self) -> usize {
        self.images.len()
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}
