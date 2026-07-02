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
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            xwindows: Vec::new(),
            is_dirty: false,
        }
    }

    /// Raise a window to the top of the stack.
    pub fn raise(&mut self, meta_window_id: u64) {
        self.windows.retain(|&id| id != meta_window_id);
        self.windows.push(meta_window_id);
        self.is_dirty = true;
    }

    /// Lower a window to the bottom of the stack.
    pub fn lower(&mut self, meta_window_id: u64) {
        self.windows.retain(|&id| id != meta_window_id);
        self.windows.insert(0, meta_window_id);
        self.is_dirty = true;
    }

    /// Restack a window relative to a sibling.
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
    pub fn add(&mut self, meta_window_id: u64) {
        if !self.windows.contains(&meta_window_id) {
            self.windows.push(meta_window_id);
            self.is_dirty = true;
        }
    }

    /// Remove a window from the stack.
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
    ///
    /// A full implementation would call XRestackWindows with the `xwindows`
    /// array so the server stacking order matches the local `windows` order.
    /// That requires a live X connection; in this no_std port we simply clear
    /// the dirty flag once the caller has performed the server sync.
    pub fn sync_to_server(&mut self) {
        if self.is_dirty {
            // A full implementation would build the XWindow array in the same
            // order as `windows` and call XRestackWindows(display, array, n).
            // The X connection is owned by the platform backend, so the actual
            // XLib call is performed there; here we record that the local and
            // server stacks are back in agreement.
            self.is_dirty = false;
        }
    }

    /// Retrieve the current stack order from the X server.
    ///
    /// A full implementation would walk the X window tree starting from the
    /// root window (XQueryTree) and rebuild `windows`/`xwindows` in the
    /// server-reported stacking order, then clear `is_dirty`. This requires a
    /// live X connection and is therefore delegated to the platform backend;
    /// without one the local stack is left unchanged.
    pub fn update_from_server(&mut self) {
        // Walking the X window tree requires XQueryTree on the root window,
        // which needs the opaque Display* handle held by the platform backend.
        // The backend would:
        //  1. Call XQueryTree(root) to obtain the child window list in
        //     bottom-to-top stacking order.
        //  2. Map each XWindow to its MetaWindow handle via the display's
        //     xid_to_window registry.
        //  3. Rebuild `xwindows` and `windows` from that traversal.
        //  4. Clear `is_dirty` since the local state now mirrors the server.
        // Without an X connection there is nothing to query, so we leave the
        // local stack untouched.
    }
}

impl Default for MetaX11Stack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_operations() {
        let mut stack = MetaX11Stack::new();
        assert!(stack.windows.is_empty());
        assert!(!stack.is_dirty);

        stack.add(1);
        assert_eq!(stack.windows.len(), 1);
        assert!(stack.is_dirty);

        stack.is_dirty = false;
        stack.add(2);
        stack.add(3);
        assert_eq!(stack.windows, vec![1, 2, 3]);

        stack.is_dirty = false;
        stack.raise(1);
        assert_eq!(stack.windows, vec![2, 3, 1]);
        assert!(stack.is_dirty);

        stack.is_dirty = false;
        stack.lower(1);
        assert_eq!(stack.windows, vec![1, 2, 3]);
        assert!(stack.is_dirty);
    }

    #[test]
    fn test_stack_restack_relative() {
        let mut stack = MetaX11Stack::new();
        stack.add(1);
        stack.add(2);
        stack.add(3);
        stack.is_dirty = false;

        stack.restack_relative(3, 1, true);
        assert_eq!(stack.windows, vec![2, 1, 3]);
        assert!(stack.is_dirty);
    }

    #[test]
    fn test_stack_position() {
        let mut stack = MetaX11Stack::new();
        stack.add(1);
        stack.add(2);
        stack.add(3);

        assert_eq!(stack.position(1), Some(0));
        assert_eq!(stack.position(2), Some(1));
        assert_eq!(stack.position(3), Some(2));
        assert_eq!(stack.position(99), None);
    }

    #[test]
    fn test_stack_top_bottom() {
        let mut stack = MetaX11Stack::new();
        assert_eq!(stack.top(), None);
        assert_eq!(stack.bottom(), None);

        stack.add(1);
        assert_eq!(stack.top(), Some(1));
        assert_eq!(stack.bottom(), Some(1));

        stack.add(2);
        assert_eq!(stack.top(), Some(2));
        assert_eq!(stack.bottom(), Some(1));
    }
}
