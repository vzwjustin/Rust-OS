//! Window stacking order (z-order) management for the desktop environment.
//!
//! This module implements core window stacking logic ported from GNOME Mutter's
//! `src/core/stack.c`. It manages the z-order (layering) of windows and provides
//! operations to raise, lower, and reorder windows while respecting layer boundaries.
//!
//! Mutter models stacking as a two-tier system:
//! - Primary sort: **layer** (Desktop < Bottom < Normal < Top/Dock < OverrideRedirect)
//! - Secondary sort: **stack position** within each layer (provides stable ordering)
//!
//! This allows windows to be moved between layers while preserving relative order
//! of sibling windows and enabling session restoration.

use crate::desktop::window_manager::WindowId;
use alloc::vec::Vec;

/// Window layer enumeration representing the z-order tier.
///
/// Windows are primarily sorted by layer; within each layer, windows are sorted
/// by stack position. This ensures that, for example, dock windows always appear
/// above normal windows regardless of their stack position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// Desktop background layer (e.g., desktop icons, wallpaper)
    Desktop = 0,

    /// Below-normal layer (e.g., panels, gadgets below normal windows)
    Bottom = 1,

    /// Normal application windows (most windows)
    Normal = 2,

    /// Top/Dock layer — taskbars, docks, panels (same as DOCK per EWMH)
    Top = 4,

    /// Override redirect windows (menus, tooltips, not managed by WM)
    OverrideRedirect = 7,
}

/// A window entry in the stack, combining identity with its layer and position.
#[derive(Debug, Clone)]
struct StackEntry {
    /// Unique window identifier
    window_id: WindowId,

    /// Current layer for this window
    layer: Layer,

    /// Unique position within the stack (used for relative ordering)
    position: u32,
}

/// Window stacking manager maintaining z-order across layers.
///
/// Manages a sorted list of windows ordered by (layer, position) tuple.
/// Supports raising/lowering windows, layer changes, and freeze/thaw
/// to batch multiple changes before applying.
pub struct Stack {
    /// Sorted vector of windows: primarily by layer, secondarily by position
    windows: Vec<StackEntry>,

    /// Freeze counter: if > 0, stack is frozen and changes are deferred
    freeze_count: i32,

    /// Number of unique positions allocated (increments for new windows)
    next_position: u32,

    /// Whether the stack needs re-sorting (set when positions change)
    need_resort: bool,

    /// Whether windows need layer recalculation
    need_relayer: bool,
}

impl Stack {
    /// Creates a new, empty window stack.
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            freeze_count: 0,
            next_position: 0,
            need_resort: false,
            need_relayer: false,
        }
    }

    /// Adds a window to the stack with the given layer.
    ///
    /// The window is inserted into the appropriate position based on its layer
    /// and assigned a unique position value for stable ordering.
    ///
    /// # Panics
    /// Panics if the window is already in the stack.
    pub fn add(&mut self, window_id: WindowId, layer: Layer) {
        assert!(
            !self.windows.iter().any(|e| e.window_id == window_id),
            "Window already in stack"
        );

        let position = self.next_position;
        self.next_position += 1;

        let entry = StackEntry {
            window_id,
            layer,
            position,
        };

        // Insert in sorted order
        let insert_pos = self
            .windows
            .iter()
            .position(|e| (e.layer, e.position) > (layer, position))
            .unwrap_or(self.windows.len());

        self.windows.insert(insert_pos, entry);
        self.need_resort = true;
    }

    /// Removes a window from the stack.
    ///
    /// # Panics
    /// Panics if the window is not in the stack.
    pub fn remove(&mut self, window_id: WindowId) {
        let idx = self
            .windows
            .iter()
            .position(|e| e.window_id == window_id)
            .expect("Window not in stack");

        self.windows.remove(idx);
        self.need_resort = true;
    }

    /// Raises a window to the top of its layer (or to a higher layer if specified).
    ///
    /// If `new_layer` is provided, the window is moved to that layer; otherwise,
    /// it is raised within its current layer.
    pub fn raise(&mut self, window_id: WindowId, new_layer: Option<Layer>) {
        let idx = self
            .windows
            .iter()
            .position(|e| e.window_id == window_id)
            .expect("Window not in stack");

        let new_layer = new_layer.unwrap_or(self.windows[idx].layer);
        let target_position = self.allocate_position();

        self.windows[idx].layer = new_layer;
        self.windows[idx].position = target_position;

        self.need_resort = true;
        self.need_relayer = new_layer != self.windows[idx].layer;
    }

    /// Lowers a window to the bottom of its layer (or to a lower layer if specified).
    ///
    /// If `new_layer` is provided, the window is moved to that layer; otherwise,
    /// it is lowered within its current layer.
    pub fn lower(&mut self, window_id: WindowId, new_layer: Option<Layer>) {
        let idx = self
            .windows
            .iter()
            .position(|e| e.window_id == window_id)
            .expect("Window not in stack");

        let new_layer = new_layer.unwrap_or(self.windows[idx].layer);

        // Allocate a position lower than any existing window in the target layer
        let target_position = self
            .windows
            .iter()
            .filter(|e| e.layer == new_layer)
            .map(|e| e.position)
            .min()
            .map(|p| p.saturating_sub(1))
            .unwrap_or(0);

        self.windows[idx].layer = new_layer;
        self.windows[idx].position = target_position;

        self.need_resort = true;
        self.need_relayer = new_layer != self.windows[idx].layer;
    }

    /// Restacks a window relative to another window.
    ///
    /// Inserts the window immediately above or below the reference window,
    /// staying within the same layer.
    pub fn restack_relative(&mut self, window_id: WindowId, relative_to: WindowId, above: bool) {
        let idx = self
            .windows
            .iter()
            .position(|e| e.window_id == window_id)
            .expect("Window not in stack");

        let ref_idx = self
            .windows
            .iter()
            .position(|e| e.window_id == relative_to)
            .expect("Reference window not in stack");

        let ref_position = self.windows[ref_idx].position;
        let target_position = if above {
            ref_position.saturating_add(1)
        } else {
            ref_position.saturating_sub(1)
        };

        self.windows[idx].position = target_position;
        self.need_resort = true;
    }

    /// Updates a window's layer, recalculating its position in the stack.
    ///
    /// Used when a window's type or properties change, requiring a layer change.
    pub fn update_layer(&mut self, window_id: WindowId, new_layer: Layer) {
        let idx = self
            .windows
            .iter()
            .position(|e| e.window_id == window_id)
            .expect("Window not in stack");

        if self.windows[idx].layer != new_layer {
            self.windows[idx].layer = new_layer;
            // Reassign position to be at the top of the new layer
            self.windows[idx].position = self.allocate_position();
            self.need_relayer = true;
            self.need_resort = true;
        }
    }

    /// Freezes the stack, deferring re-sorting until thawed.
    ///
    /// Useful for batching multiple changes before applying the re-sort.
    pub fn freeze(&mut self) {
        self.freeze_count += 1;
    }

    /// Thaws the stack, applying deferred changes when fully thawed.
    ///
    /// Re-sorts the stack if `freeze_count` reaches 0 and changes were made.
    pub fn thaw(&mut self) {
        if self.freeze_count > 0 {
            self.freeze_count -= 1;
            if self.freeze_count == 0 && self.need_resort {
                self.apply_resort();
            }
        }
    }

    /// Returns true if the stack is currently frozen.
    pub fn is_frozen(&self) -> bool {
        self.freeze_count > 0
    }

    /// Returns the number of windows in the stack.
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    /// Returns true if the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Returns the topmost window in the stack, or None if empty.
    pub fn get_top(&self) -> Option<WindowId> {
        self.windows.last().map(|e| e.window_id)
    }

    /// Returns the window above the specified window, or None if at top.
    pub fn get_above(&self, window_id: WindowId) -> Option<WindowId> {
        let idx = self.windows.iter().position(|e| e.window_id == window_id)?;
        self.windows.get(idx + 1).map(|e| e.window_id)
    }

    /// Returns the window below the specified window, or None if at bottom.
    pub fn get_below(&self, window_id: WindowId) -> Option<WindowId> {
        let idx = self.windows.iter().position(|e| e.window_id == window_id)?;
        if idx == 0 {
            None
        } else {
            self.windows.get(idx - 1).map(|e| e.window_id)
        }
    }

    /// Returns an iterator over window IDs in stacking order (bottom to top).
    pub fn iter(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.windows.iter().map(|e| e.window_id)
    }

    /// Allocates a new unique position value for a window being raised.
    fn allocate_position(&mut self) -> u32 {
        let pos = self.next_position;
        self.next_position = pos.wrapping_add(1);
        pos
    }

    /// Applies deferred re-sorting to the windows vector.
    fn apply_resort(&mut self) {
        self.windows.sort_by_key(|e| (e.layer, e.position));
        self.need_resort = false;
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_remove() {
        let mut stack = Stack::new();
        let w1 = WindowId::from_raw(1);
        let w2 = WindowId::from_raw(2);

        stack.add(w1, Layer::Normal);
        stack.add(w2, Layer::Normal);

        assert_eq!(stack.len(), 2);

        stack.remove(w1);
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn test_layer_ordering() {
        let mut stack = Stack::new();
        let w_desktop = WindowId::from_raw(1);
        let w_normal = WindowId::from_raw(2);
        let w_dock = WindowId::from_raw(3);

        stack.add(w_normal, Layer::Normal);
        stack.add(w_desktop, Layer::Desktop);
        stack.add(w_dock, Layer::Top);

        let order: Vec<_> = stack.iter().collect();
        assert_eq!(order, vec![w_desktop, w_normal, w_dock]);
    }

    #[test]
    fn test_raise_and_lower() {
        let mut stack = Stack::new();
        let w1 = WindowId::from_raw(1);
        let w2 = WindowId::from_raw(2);

        stack.add(w1, Layer::Normal);
        stack.add(w2, Layer::Normal);

        stack.raise(w1, None);
        assert_eq!(stack.get_top(), Some(w1));

        stack.lower(w1, None);
        assert_eq!(stack.get_top(), Some(w2));
    }

    #[test]
    fn test_freeze_thaw() {
        let mut stack = Stack::new();
        let w1 = WindowId::from_raw(1);

        stack.freeze();
        assert!(stack.is_frozen());

        stack.add(w1, Layer::Normal);
        assert_eq!(stack.len(), 1);

        stack.thaw();
        assert!(!stack.is_frozen());
    }
}
