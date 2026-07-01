//! Compositor view ported from `meta-compositor-view.c`.
//!
//! Manages rendering views (typically one per monitor).

use crate::desktop::window_manager::WindowId;
use alloc::vec::Vec;

/// Rendering view (typically maps to a physical display)
#[derive(Debug)]
pub struct CompositorView {
    pub id: u32,
    pub stage_view_id: u32,
    pub top_window: Option<WindowId>,
    pub window_actors: Vec<WindowId>,
}

impl CompositorView {
    /// Create new compositor view
    pub fn new(id: u32, stage_view_id: u32) -> Self {
        CompositorView {
            id,
            stage_view_id,
            top_window: None,
            window_actors: Vec::new(),
        }
    }

    /// Update top-most window actor
    pub fn update_top_window_actor(&mut self, window_id: Option<WindowId>) {
        self.top_window = window_id;
    }

    /// Get top window actor
    pub fn get_top_window_actor(&self) -> Option<WindowId> {
        self.top_window
    }

    /// Add window to this view
    pub fn add_window(&mut self, window_id: WindowId) {
        if !self.window_actors.contains(&window_id) {
            self.window_actors.push(window_id);
        }
    }

    /// Remove window from this view
    pub fn remove_window(&mut self, window_id: WindowId) {
        self.window_actors.retain(|&id| id != window_id);
    }

    /// Get windows in this view
    pub fn get_windows(&self) -> &[WindowId] {
        &self.window_actors
    }
}
