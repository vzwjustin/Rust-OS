//! Workspace management ported from GNOME Mutter (src/core/workspace.c).
//!
//! Implements the core workspace data model: each workspace holds a set of windows,
//! tracks which workspace is active, and provides operations to add/remove windows
//! and switch between workspaces.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/workspace.c
//! Omitted: GObject signal machinery, X11/Wayland window operations, ACPI/logical monitor
//! layout calculations, desktop-switch sound effects, ATK accessibility hooks.

use crate::desktop::window_manager::WindowId;
use alloc::vec::Vec;

/// Unique identifier for a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkspaceId(pub usize);

/// Represents a single workspace holding a collection of windows.
#[derive(Debug)]
pub struct Workspace {
    /// Unique identifier for this workspace.
    id: WorkspaceId,
    /// Index in the workspace grid (0-based).
    index: usize,
    /// Windows present on this workspace (MRU order: most recent first).
    windows: Vec<WindowId>,
    /// Whether this is the currently active workspace.
    active: bool,
}

impl Workspace {
    /// Create a new workspace with the given index.
    pub fn new(id: WorkspaceId, index: usize) -> Self {
        Workspace {
            id,
            index,
            windows: Vec::new(),
            active: false,
        }
    }

    /// Get this workspace's unique identifier.
    pub fn id(&self) -> WorkspaceId {
        self.id
    }

    /// Get this workspace's index in the grid.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Return whether this is the active workspace.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Mark this workspace as active or inactive.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Get the number of windows on this workspace.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Add a window to this workspace (prepends for MRU tracking).
    pub fn add_window(&mut self, window_id: WindowId) {
        if !self.windows.contains(&window_id) {
            self.windows.insert(0, window_id);
        }
    }

    /// Remove a window from this workspace.
    pub fn remove_window(&mut self, window_id: WindowId) {
        self.windows.retain(|&id| id != window_id);
    }

    /// Check if a window is on this workspace.
    pub fn contains_window(&self, window_id: WindowId) -> bool {
        self.windows.contains(&window_id)
    }

    /// Get a copy of all window IDs on this workspace (MRU order).
    pub fn windows(&self) -> Vec<WindowId> {
        self.windows.clone()
    }

    /// Move a window to the front of the MRU list (most recently used).
    pub fn focus_window(&mut self, window_id: WindowId) {
        if let Some(pos) = self.windows.iter().position(|&id| id == window_id) {
            self.windows.remove(pos);
            self.windows.insert(0, window_id);
        }
    }
}

/// Manages a collection of workspaces.
#[derive(Debug)]
pub struct WorkspaceManager {
    /// All workspaces indexed by their WorkspaceId.
    workspaces: Vec<Workspace>,
    /// Index of the currently active workspace.
    active_index: usize,
}

impl WorkspaceManager {
    /// Create a new workspace manager with the given number of workspaces.
    pub fn new(count: usize) -> Self {
        let mut workspaces = Vec::with_capacity(count);
        for i in 0..count {
            workspaces.push(Workspace::new(WorkspaceId(i), i));
        }
        WorkspaceManager {
            workspaces,
            active_index: 0,
        }
    }

    /// Get the total number of workspaces.
    pub fn workspace_count(&self) -> usize {
        self.workspaces.len()
    }

    /// Get the index of the active workspace.
    pub fn active_workspace_index(&self) -> usize {
        self.active_index
    }

    /// Get a reference to the active workspace.
    pub fn active_workspace(&self) -> &Workspace {
        &self.workspaces[self.active_index]
    }

    /// Get a mutable reference to the active workspace.
    pub fn active_workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active_index]
    }

    /// Get a reference to a workspace by index.
    pub fn workspace(&self, index: usize) -> Option<&Workspace> {
        self.workspaces.get(index)
    }

    /// Get a mutable reference to a workspace by index.
    pub fn workspace_mut(&mut self, index: usize) -> Option<&mut Workspace> {
        self.workspaces.get_mut(index)
    }

    /// Switch to a workspace by index. Returns true if successful.
    pub fn activate_workspace(&mut self, index: usize) -> bool {
        if index >= self.workspaces.len() {
            return false;
        }
        if index == self.active_index {
            return true;
        }

        self.workspaces[self.active_index].set_active(false);
        self.active_index = index;
        self.workspaces[self.active_index].set_active(true);
        true
    }

    /// Add a window to a workspace by index.
    pub fn add_window_to_workspace(&mut self, window_id: WindowId, workspace_index: usize) -> bool {
        match self.workspaces.get_mut(workspace_index) {
            Some(ws) => {
                ws.add_window(window_id);
                true
            }
            None => false,
        }
    }

    /// Remove a window from a workspace by index.
    pub fn remove_window_from_workspace(
        &mut self,
        window_id: WindowId,
        workspace_index: usize,
    ) -> bool {
        match self.workspaces.get_mut(workspace_index) {
            Some(ws) => {
                ws.remove_window(window_id);
                true
            }
            None => false,
        }
    }

    /// Remove a window from all workspaces.
    pub fn remove_window(&mut self, window_id: WindowId) {
        for ws in &mut self.workspaces {
            ws.remove_window(window_id);
        }
    }

    /// Find which workspace(s) contain the given window.
    /// Returns a Vec of workspace indices.
    pub fn find_workspaces_with_window(&self, window_id: WindowId) -> Vec<usize> {
        self.workspaces
            .iter()
            .enumerate()
            .filter(|(_, ws)| ws.contains_window(window_id))
            .map(|(i, _)| i)
            .collect()
    }

    /// Move a window to a different workspace. Removes from old, adds to new.
    pub fn move_window_to_workspace(&mut self, window_id: WindowId, target_index: usize) -> bool {
        if target_index >= self.workspaces.len() {
            return false;
        }
        self.remove_window(window_id);
        self.add_window_to_workspace(window_id, target_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_creation() {
        let ws = Workspace::new(WorkspaceId(0), 0);
        assert_eq!(ws.index(), 0);
        assert_eq!(ws.window_count(), 0);
        assert!(!ws.is_active());
    }

    #[test]
    fn test_workspace_manager_creation() {
        let mgr = WorkspaceManager::new(4);
        assert_eq!(mgr.workspace_count(), 4);
        assert_eq!(mgr.active_workspace_index(), 0);
        assert!(mgr.active_workspace().is_active());
    }

    #[test]
    fn test_add_window() {
        let mut ws = Workspace::new(WorkspaceId(0), 0);
        let win_id = WindowId(1);
        ws.add_window(win_id);
        assert_eq!(ws.window_count(), 1);
        assert!(ws.contains_window(win_id));
    }

    #[test]
    fn test_remove_window() {
        let mut ws = Workspace::new(WorkspaceId(0), 0);
        let win_id = WindowId(1);
        ws.add_window(win_id);
        ws.remove_window(win_id);
        assert_eq!(ws.window_count(), 0);
        assert!(!ws.contains_window(win_id));
    }

    #[test]
    fn test_activate_workspace() {
        let mut mgr = WorkspaceManager::new(4);
        assert_eq!(mgr.active_workspace_index(), 0);
        mgr.activate_workspace(2);
        assert_eq!(mgr.active_workspace_index(), 2);
        assert!(mgr.workspaces[2].is_active());
        assert!(!mgr.workspaces[0].is_active());
    }

    #[test]
    fn test_move_window_between_workspaces() {
        let mut mgr = WorkspaceManager::new(2);
        let win_id = WindowId(1);
        mgr.add_window_to_workspace(win_id, 0);
        assert!(mgr.workspaces[0].contains_window(win_id));
        assert!(!mgr.workspaces[1].contains_window(win_id));

        mgr.move_window_to_workspace(win_id, 1);
        assert!(!mgr.workspaces[0].contains_window(win_id));
        assert!(mgr.workspaces[1].contains_window(win_id));
    }

    #[test]
    fn test_mru_ordering() {
        let mut ws = Workspace::new(WorkspaceId(0), 0);
        ws.add_window(WindowId(1));
        ws.add_window(WindowId(2));
        ws.add_window(WindowId(3));

        let windows = ws.windows();
        assert_eq!(windows[0], WindowId(3)); // Most recent first

        ws.focus_window(WindowId(1));
        let windows = ws.windows();
        assert_eq!(windows[0], WindowId(1)); // Now first after focus
    }
}
