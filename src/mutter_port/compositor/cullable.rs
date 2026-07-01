//! Cullable interface ported from `meta-cullable.c`.
//!
//! Provides occlusion culling to optimize rendering by skipping invisible regions.

use alloc::vec::Vec;

/// Region tracking for culling operations
#[derive(Debug, Clone)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    /// Create new region
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Region {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if region is empty
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Merge with another region
    pub fn merge(&mut self, other: &Region) {
        let min_x = self.x.min(other.x);
        let min_y = self.y.min(other.y);
        let max_x = (self.x + self.width).max(other.x + other.width);
        let max_y = (self.y + self.height).max(other.y + other.height);

        self.x = min_x;
        self.y = min_y;
        self.width = max_x - min_x;
        self.height = max_y - min_y;
    }

    /// Check intersection with another region
    pub fn intersects(&self, other: &Region) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

/// Trait for objects that support culling optimization
pub trait Cullable {
    /// Cull unobscured (visible) regions
    fn cull_unobscured(&self, unobscured: &mut Vec<Region>);

    /// Cull redraw clip regions
    fn cull_redraw_clip(&self, clip: &mut Vec<Region>);
}

/// Helper to cull unobscured children
pub fn cull_unobscured_children(children: &[&dyn Cullable], unobscured: &mut Vec<Region>) {
    for child in children {
        child.cull_unobscured(unobscured);
    }
}

/// Helper to cull redraw clip of children
pub fn cull_redraw_clip_children(children: &[&dyn Cullable], clip: &mut Vec<Region>) {
    for child in children {
        child.cull_redraw_clip(clip);
    }
}
