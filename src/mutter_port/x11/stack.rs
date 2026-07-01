//! X11 window stacking order.
//!
//! Ported from GNOME Mutter's src/x11/meta-x11-stack.c/.h.
//! Manages the Z-order of windows, both for X11 stacking and internal tracking.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-x11-stack.c

use crate::mutter_port::x11::display::XWindow;
use alloc::vec::Vec;

/// Represents the window stacking order.
pub struct MetaX11Stack {
    /// Stack of window IDs from bottom to top.
    pub windows: Vec<u64>, // MetaWindow handles

    /// X window ID stack for syncing with X server.
    pub xwindows: Vec<XWindow>,

    /// Whether the local stack differs from the X server stack.
    pub is_dirty: bool,
}

impl MetaX11Stack {
    /// Create a new window stack.
    /// # TODO: port logic from meta_x11_stack_new()
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            xwindows: Vec::new(),
            is_dirty: false,
        }
    }

    /// Raise a window to the top of the stack.
    /// # TODO: port logic from meta_x11_stack_raise()
    pub fn raise(&mut self, meta_window_id: u64) {
        self.windows.retain(|&id| id != meta_window_id);
        self.windows.push(meta_window_id);
        self.is_dirty = true;
    }

    /// Lower a window to the bottom of the stack.
    /// # TODO: port logic from meta_x11_stack_lower()
    pub fn lower(&mut self, meta_window_id: u64) {
        self.windows.retain(|&id| id != meta_window_id);
        self.windows.insert(0, meta_window_id);
        self.is_dirty = true;
    }

    /// Restack a window relative to a sibling.
    /// # TODO: port logic from meta_x11_stack_restack()
    pub fn restack_relative(&mut self, meta_window_id: u64, sibling_id: u64, above: bool) {
        self.windows.retain(|&id| id != meta_window_id);
        if let Some(sibling_idx) = self.windows.iter().position(|&id| id == sibling_id) {
            let new_idx = if above { sibling_idx + 1 } else { sibling_idx };
            self.windows.insert(new_idx, meta_window_id);
        } else {
            self.windows.push(meta_window_id);
        }
        self.is_dirty = true;
    }

    /// Add a window to the stack.
    /// # TODO: port logic from meta_x11_stack_add()
    pub fn add(&mut self, meta_window_id: u64) {
        if !self.windows.contains(&meta_window_id) {
            self.windows.push(meta_window_id);
            self.is_dirty = true;
        }
    }

    /// Remove a window from the stack.
    /// # TODO: port logic from meta_x11_stack_remove()
    pub fn remove(&mut self, meta_window_id: u64) {
        let before = self.windows.len();
        self.windows.retain(|&id| id != meta_window_id);
        if self.windows.len() != before {
            self.is_dirty = true;
        }
    }

    /// Get the window at the top of the stack.
    pub fn top(&self) -> Option<u64> {
        self.windows.last().copied()
    }

    /// Get the window at the bottom of the stack.
    pub fn bottom(&self) -> Option<u64> {
        self.windows.first().copied()
    }

    /// Get the position of a window in the stack.
    pub fn position(&self, meta_window_id: u64) -> Option<usize> {
        self.windows.iter().position(|&id| id == meta_window_id)
    }

    /// Get all windows in stacking order.
    pub fn get_windows(&self) -> &[u64] {
        &self.windows
    }

    /// Sync the local stack to the X server.
    /// # TODO: port logic from meta_x11_stack_sync_to_server()
    pub fn sync_to_server(&mut self) {
        if self.is_dirty {
            // TODO: call XRestackWindows with xwindows array
            self.is_dirty = false;
        }
    }

    /// Retrieve the current stack order from the X server.
    /// # TODO: port logic from meta_x11_stack_update_from_server()
    pub fn update_from_server(&mut self) {
        // TODO: walk the X window tree and rebuild stack
    }
}

impl Default for MetaX11Stack {
    fn default() -> Self {
        Self::new()
    }
}
