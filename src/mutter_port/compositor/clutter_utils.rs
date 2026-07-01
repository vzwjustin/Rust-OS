//! Clutter rendering utilities ported from GNOME Mutter's `src/compositor/clutter-utils.c`.
//!
//! Provides utility functions for transformations, fixed-point arithmetic,
//! and actor painting operations compatible with Clutter's rendering model.

use core::f32;

/// Fixed-point precision (8-bit fractional part)
const FIXED_SHIFT: u32 = 8;
const FIXED_ONE: i32 = 1 << FIXED_SHIFT;

/// Convert floating-point value to fixed-point representation
pub fn float_to_fixed(x: f32) -> i32 {
    libm::roundf(x * FIXED_ONE as f32) as i32
}

/// Convert fixed-point value to floating-point
pub fn fixed_to_float(x: i32) -> f32 {
    x as f32 / FIXED_ONE as f32
}

/// Represents a 2D transformation matrix for actor positioning/rotation
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub scale_x: f32,
    pub scale_y: f32,
    pub translate_x: f32,
    pub translate_y: f32,
    pub rotation: f32,
    pub depth: f32,
}

impl Transform {
    /// Identity transformation (no changes)
    pub fn identity() -> Self {
        Transform {
            scale_x: 1.0,
            scale_y: 1.0,
            translate_x: 0.0,
            translate_y: 0.0,
            rotation: 0.0,
            depth: 0.0,
        }
    }

    /// Check if transformation is axis-aligned (no rotation/skew)
    pub fn is_axis_aligned(&self) -> bool {
        self.rotation == 0.0
    }

    /// Check if actor is visible after transformation
    pub fn is_visible(&self) -> bool {
        self.scale_x > 0.0 && self.scale_y > 0.0
    }
}

/// Vertex data for painting operations
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vertex {
    /// Create a new 2D vertex (z=0)
    pub fn new(x: f32, y: f32) -> Self {
        Vertex { x, y, z: 0.0 }
    }

    /// Create a new 3D vertex
    pub fn new_3d(x: f32, y: f32, z: f32) -> Self {
        Vertex { x, y, z }
    }
}

/// Rectangle bounds for painting
#[derive(Debug, Clone, Copy)]
pub struct PaintBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PaintBounds {
    /// Create new paint bounds
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        PaintBounds {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if point is within bounds
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }

    /// Calculate vertices for quad covering these bounds
    pub fn vertices(&self) -> [Vertex; 4] {
        [
            Vertex::new(self.x, self.y),
            Vertex::new(self.x + self.width, self.y),
            Vertex::new(self.x + self.width, self.y + self.height),
            Vertex::new(self.x, self.y + self.height),
        ]
    }
}

/// Check if actor painting is axis-aligned (not transformed)
pub fn actor_painting_untransformed(transform: &Transform) -> bool {
    transform.is_axis_aligned() && transform.depth == 0.0
}

/// Convert viewport coordinates from Clutter space to OpenGL space
pub fn clutter_to_opengl_coords(x: f32, y: f32, width: f32, height: f32) -> (f32, f32) {
    // Clutter Y is top-down, OpenGL is bottom-up
    (x, height - y)
}

/// Convert OpenGL coordinates back to Clutter space
pub fn opengl_to_clutter_coords(x: f32, y: f32, width: f32, height: f32) -> (f32, f32) {
    (x, height - y)
}
