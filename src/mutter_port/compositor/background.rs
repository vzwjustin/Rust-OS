//! Desktop background management ported from GNOME Mutter's `src/compositor/meta-background.c`.
//!
//! Handles background images, colors, and rendering for the desktop environment.

use crate::graphics::framebuffer::Color;
use alloc::vec::Vec;

/// Background blend mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundBlendMode {
    /// Solid color background
    Solid,
    /// Gradient background (top to bottom)
    Gradient,
    /// Tiled image background
    Tiled,
    /// Scaled/stretched image background
    Scaled,
    /// Centered image with borders
    Centered,
}

/// Background source (image or color)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundSource {
    /// Solid color (RGB)
    Color(Color),
    /// Image file path
    Image(usize), // Image ID reference
    /// Gradient from one color to another
    Gradient(Color, Color),
}

/// Desktop background configuration
#[derive(Debug, Clone)]
pub struct Background {
    /// Visual content source
    pub source: BackgroundSource,
    /// How to render the background
    pub blend_mode: BackgroundBlendMode,
    /// Display width/height
    pub width: u32,
    pub height: u32,
    /// Animation/update flag
    pub animated: bool,
    /// Opacity (0.0 = transparent, 1.0 = opaque)
    pub opacity: f32,
}

impl Background {
    /// Create new solid color background
    pub fn solid(color: Color, width: u32, height: u32) -> Self {
        Background {
            source: BackgroundSource::Color(color),
            blend_mode: BackgroundBlendMode::Solid,
            width,
            height,
            animated: false,
            opacity: 1.0,
        }
    }

    /// Create new gradient background
    pub fn gradient(top: Color, bottom: Color, width: u32, height: u32) -> Self {
        Background {
            source: BackgroundSource::Gradient(top, bottom),
            blend_mode: BackgroundBlendMode::Gradient,
            width,
            height,
            animated: false,
            opacity: 1.0,
        }
    }

    /// Create new image-based background
    pub fn image(image_id: usize, width: u32, height: u32, blend: BackgroundBlendMode) -> Self {
        Background {
            source: BackgroundSource::Image(image_id),
            blend_mode: blend,
            width,
            height,
            animated: false,
            opacity: 1.0,
        }
    }

    /// Set background opacity
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.max(0.0).min(1.0);
    }

    /// Check if background needs animation updates
    pub fn is_animated(&self) -> bool {
        self.animated
    }

    /// Update background for animation frame
    pub fn update_animated_frame(&mut self) {
        // Placeholder for animation updates
    }

    /// Get effective color for rendering (used for gradients and solid colors)
    pub fn get_color(&self) -> Color {
        match &self.source {
            BackgroundSource::Color(c) => *c,
            BackgroundSource::Gradient(top, _) => *top, // Return top color as primary
            BackgroundSource::Image(_) => Color::rgb(0, 0, 0), // Black for images
        }
    }
}

impl Default for Background {
    fn default() -> Self {
        Background {
            source: BackgroundSource::Color(Color::rgb(0, 0, 0)),
            blend_mode: BackgroundBlendMode::Solid,
            width: 1920,
            height: 1080,
            animated: false,
            opacity: 1.0,
        }
    }
}

/// Background manager (tracks system backgrounds)
pub struct BackgroundManager {
    backgrounds: Vec<Background>,
    active_index: usize,
}

impl BackgroundManager {
    /// Create new background manager
    pub fn new() -> Self {
        BackgroundManager {
            backgrounds: Vec::new(),
            active_index: 0,
        }
    }

    /// Add a background to the manager
    pub fn add_background(&mut self, background: Background) -> usize {
        let index = self.backgrounds.len();
        self.backgrounds.push(background);
        index
    }

    /// Get active background
    pub fn get_active(&self) -> Option<&Background> {
        self.backgrounds.get(self.active_index)
    }

    /// Get mutable reference to active background
    pub fn get_active_mut(&mut self) -> Option<&mut Background> {
        self.backgrounds.get_mut(self.active_index)
    }

    /// Set active background by index
    pub fn set_active(&mut self, index: usize) -> bool {
        if index < self.backgrounds.len() {
            self.active_index = index;
            true
        } else {
            false
        }
    }

    /// Get background by index
    pub fn get(&self, index: usize) -> Option<&Background> {
        self.backgrounds.get(index)
    }

    /// Get mutable reference by index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Background> {
        self.backgrounds.get_mut(index)
    }

    /// Get number of backgrounds
    pub fn count(&self) -> usize {
        self.backgrounds.len()
    }

    /// Update animated backgrounds
    pub fn update_animations(&mut self) {
        for bg in &mut self.backgrounds {
            if bg.is_animated() {
                bg.update_animated_frame();
            }
        }
    }
}

impl Default for BackgroundManager {
    fn default() -> Self {
        Self::new()
    }
}
