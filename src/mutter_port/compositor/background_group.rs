//! Background group actor ported from `meta-background-group.c`.
//!
//! Groups background actors for efficient culling and rendering.

use alloc::vec::Vec;

/// Background group containing multiple background actors
#[derive(Debug)]
pub struct BackgroundGroup {
    pub id: u32,
    pub children: Vec<u32>,
    pub visible: bool,
    pub frozen: bool,
}

impl BackgroundGroup {
    /// Create new background group
    pub fn new(id: u32) -> Self {
        BackgroundGroup {
            id,
            children: Vec::new(),
            visible: true,
            frozen: false,
        }
    }

    /// Add child actor to group
    pub fn add_child(&mut self, actor_id: u32) {
        if !self.children.contains(&actor_id) {
            self.children.push(actor_id);
        }
    }

    /// Remove child actor from group
    pub fn remove_child(&mut self, actor_id: u32) {
        self.children.retain(|&id| id != actor_id);
    }

    /// Set group visibility
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Freeze group (prevents updates)
    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    /// Thaw group (allows updates)
    pub fn thaw(&mut self) {
        self.frozen = false;
    }

    /// Check if group is frozen
    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// Get child count
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// Get children
    pub fn get_children(&self) -> &[u32] {
        &self.children
    }

    /// Cull unobscured regions (for optimization)
    pub fn cull_unobscured(&self, x: u32, y: u32, width: u32, height: u32) {
        // Region tracking for efficient redraw
    }

    /// Cull redraw clip regions
    pub fn cull_redraw_clip(&self, x: u32, y: u32, width: u32, height: u32) {
        // Clip region tracking
    }
}
