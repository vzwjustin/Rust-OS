//! Cogl (GPU graphics library) utilities ported from GNOME Mutter's `src/compositor/cogl-utils.c`.
//!
//! Provides GPU texture pipeline management and rendering primitives.
//! Adapts Cogl's GPU abstraction layer concepts to RustOS's graphics infrastructure.

use crate::graphics::framebuffer::Color;
use alloc::vec::Vec;

/// GPU texture components (color, alpha, or depth)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureComponents {
    /// Color RGB data
    Rgb,
    /// Color RGBA with alpha channel
    Rgba,
    /// Red and Green channels only
    Rg,
    /// Alpha channel only
    A,
    /// Depth data (for shadow maps, etc)
    Depth,
}

/// Texture allocation flags
#[derive(Debug, Clone, Copy)]
pub struct TextureFlags {
    /// Allow the texture to be sliced if it exceeds hardware limits
    pub allow_slicing: bool,
    /// Texture can be modified after creation
    pub dynamic: bool,
    /// Texture is only readable, not writable
    pub immutable: bool,
}

impl Default for TextureFlags {
    fn default() -> Self {
        TextureFlags {
            allow_slicing: false,
            dynamic: false,
            immutable: false,
        }
    }
}

/// GPU texture object
#[derive(Debug, Clone)]
pub struct Texture {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub components: TextureComponents,
    pub flags: TextureFlags,
}

impl Texture {
    /// Create a new texture with specified dimensions and components
    pub fn new(id: u32, width: u32, height: u32, components: TextureComponents) -> Self {
        Texture {
            id,
            width,
            height,
            components,
            flags: TextureFlags::default(),
        }
    }

    /// Get the memory size of this texture in bytes
    pub fn memory_size(&self) -> usize {
        let components_per_pixel = match self.components {
            TextureComponents::Rgb => 3,
            TextureComponents::Rgba => 4,
            TextureComponents::Rg => 2,
            TextureComponents::A => 1,
            TextureComponents::Depth => 4,
        };
        (self.width as usize) * (self.height as usize) * components_per_pixel
    }

    /// Check if this texture is at maximum hardware limits
    pub fn at_hardware_limits(&self) -> bool {
        // Typical GPU max is 16384x16384 for modern hardware
        self.width > 16384 || self.height > 16384
    }
}

/// GPU texture pipeline/shader program
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub id: u32,
    pub layers: Vec<TextureLayer>,
}

/// Texture layer in a pipeline (typically for multi-texturing)
#[derive(Debug, Clone)]
pub struct TextureLayer {
    pub texture: Option<Texture>,
    pub wrap_s: WrapMode,
    pub wrap_t: WrapMode,
    pub mag_filter: FilterMode,
    pub min_filter: FilterMode,
}

/// Texture wrapping mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapMode {
    /// Clamp edges to boundary colors
    Clamp,
    /// Repeat texture (tile)
    Repeat,
    /// Mirrored repeat
    MirroredRepeat,
}

impl Default for WrapMode {
    fn default() -> Self {
        WrapMode::Clamp
    }
}

/// Texture filtering mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    /// Nearest neighbor (fastest, pixelated)
    Nearest,
    /// Linear interpolation (smooth)
    Linear,
    /// Tri-linear with mipmaps (best quality)
    Trilinear,
}

impl Default for FilterMode {
    fn default() -> Self {
        FilterMode::Linear
    }
}

impl Pipeline {
    /// Create a new texture pipeline
    pub fn new(id: u32) -> Self {
        Pipeline {
            id,
            layers: Vec::new(),
        }
    }

    /// Create a pipeline with a single texture layer
    pub fn with_texture(id: u32, texture: Texture) -> Self {
        let layer = TextureLayer {
            texture: Some(texture),
            wrap_s: WrapMode::default(),
            wrap_t: WrapMode::default(),
            mag_filter: FilterMode::default(),
            min_filter: FilterMode::default(),
        };

        let mut layers = Vec::new();
        layers.push(layer);

        Pipeline { id, layers }
    }

    /// Add a texture layer to this pipeline
    pub fn add_layer(&mut self, texture: Texture) {
        let layer = TextureLayer {
            texture: Some(texture),
            wrap_s: WrapMode::default(),
            wrap_t: WrapMode::default(),
            mag_filter: FilterMode::default(),
            min_filter: FilterMode::default(),
        };
        self.layers.push(layer);
    }

    /// Get the first texture layer (most common use case)
    pub fn first_texture(&self) -> Option<&Texture> {
        self.layers.first().and_then(|layer| layer.texture.as_ref())
    }
}

/// Blend mode for rendering operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// Simple alpha blending
    Alpha,
    /// Additive blending (accumulates)
    Add,
    /// Multiply blending (darkens)
    Multiply,
    /// No blending (opaque)
    Replace,
}

impl Default for BlendMode {
    fn default() -> Self {
        BlendMode::Alpha
    }
}

/// Create a new texture with specified properties
pub fn create_texture(width: u32, height: u32, components: TextureComponents) -> Texture {
    // In a real implementation, this would allocate GPU memory
    // For now, return a stub texture object
    Texture::new(0, width, height, components)
}

/// Create a rendering pipeline template
pub fn create_pipeline() -> Pipeline {
    Pipeline::new(0)
}

/// Check if GPU supports a given texture size
pub fn supports_texture_size(width: u32, height: u32) -> bool {
    width > 0 && height > 0 && width <= 16384 && height <= 16384
}
