//! Drag and drop actor for visual feedback ported from `meta-dnd-actor.c`.
//!
//! Renders drag source cursor and feedback during drag operations.

use crate::graphics::framebuffer::Color;

/// Actor for drag operation visual feedback
#[derive(Debug)]
pub struct DndActor {
    pub id: u32,
    pub visible: bool,
    pub x: i32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub opacity: f32,
}

impl DndActor {
    /// Create new DnD actor
    pub fn new(id: u32) -> Self {
        DndActor {
            id,
            visible: false,
            x: 0,
            y: 0,
            width: 32,
            height: 32,
            opacity: 1.0,
        }
    }

    /// Show drag cursor
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide drag cursor
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Update cursor position
    pub fn set_position(&mut self, x: i32, y: u32) {
        self.x = x;
        self.y = y;
    }

    /// Set cursor size
    pub fn set_size(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    /// Set opacity for drag feedback
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.max(0.0).min(1.0);
    }

    /// Paint drag feedback
    pub fn paint(&self) {
        if !self.visible || self.opacity == 0.0 {
            return;
        }
        // Render drag cursor at (x, y) with size (width, height)
    }
}
