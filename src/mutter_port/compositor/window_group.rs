//! Window group compositing layer manager.
//! Ported from Mutter: /home/justin/Downloads/mutter-main/src/compositor/meta-window-group.c
//! A window group represents an ordered collection of windows in a single compositing layer.

use crate::desktop::window_manager::WindowId;
use alloc::vec::Vec;

/// A compositing layer group containing an ordered list of window actors.
/// Each group represents windows at a specific z-order level (e.g., top, bottom).
pub struct WindowGroup {
    windows: Vec<WindowId>,
}

impl WindowGroup {
    /// Create a new, empty window group.
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
        }
    }

    /// Add a window to the group (no-op if already present).
    pub fn add(&mut self, window_id: WindowId) {
        if !self.windows.contains(&window_id) {
            self.windows.push(window_id);
        }
    }

    /// Remove a window from the group.
    pub fn remove(&mut self, window_id: WindowId) {
        self.windows.retain(|&id| id != window_id);
    }

    /// Reorder a window to a specific position in the group's z-order.
    pub fn reorder(&mut self, window_id: WindowId, position: usize) {
        if let Some(idx) = self.windows.iter().position(|&id| id == window_id) {
            self.windows.remove(idx);
            let new_pos = position.min(self.windows.len());
            self.windows.insert(new_pos, window_id);
        }
    }

    /// Get a reference to the windows in z-order.
    pub fn windows(&self) -> &[WindowId] {
        &self.windows
    }

    /// Get a mutable reference to the windows vector.
    pub fn windows_mut(&mut self) -> &mut Vec<WindowId> {
        &mut self.windows
    }

    /// Get the number of windows in the group.
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    /// Check if the group is empty.
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Clear all windows from the group.
    pub fn clear(&mut self) {
        self.windows.clear();
    }

    /// Iterate over window IDs in z-order.
    pub fn iter(&self) -> impl Iterator<Item = &WindowId> {
        self.windows.iter()
    }
}

impl Default for WindowGroup {
    fn default() -> Self {
        Self::new()
    }
}
