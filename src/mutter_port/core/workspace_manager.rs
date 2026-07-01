//! Workspace manager ported from GNOME Mutter's src/core/meta-workspace-manager.c
//!
//! Implements MetaWorkspaceManager which manages a collection of workspaces,
//! handles workspace switching, and tracks which windows are on which workspaces.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-workspace-manager.c

use crate::desktop::window_manager::WindowId;
use alloc::vec::Vec;

/// Unique identifier for a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceId(pub usize);

/// Represents a single workspace.
#[derive(Debug)]
pub struct Workspace {
    /// Unique workspace identifier.
    id: WorkspaceId,
    /// Index in workspace grid (0-based).
    index: usize,
    /// Windows on this workspace (MRU order).
    windows: Vec<WindowId>,
    /// Whether this is the active workspace.
    active: bool,
    /// Workspace name.
    name: alloc::string::String,
}

impl Workspace {
    /// Create a new workspace.
    pub fn new(id: WorkspaceId, index: usize, name: alloc::string::String) -> Self {
        Workspace {
            id,
            index,
            windows: Vec::new(),
            active: false,
            name,
        }
    }

    /// Get workspace ID.
    pub fn id(&self) -> WorkspaceId {
        self.id
    }

    /// Get workspace index.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Get workspace name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set workspace name.
    pub fn set_name(&mut self, name: alloc::string::String) {
        self.name = name;
    }

    /// Check if workspace is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Set active state.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Get number of windows on this workspace.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Add a window to this workspace.
    pub fn add_window(&mut self, window_id: WindowId) {
        if !self.windows.contains(&window_id) {
            self.windows.insert(0, window_id);
        }
    }

    /// Remove a window from this workspace.
    pub fn remove_window(&mut self, window_id: WindowId) {
        self.windows.retain(|&id| id != window_id);
    }

    /// Check if workspace contains window.
    pub fn contains_window(&self, window_id: WindowId) -> bool {
        self.windows.contains(&window_id)
    }

    /// Get all windows on this workspace (MRU order).
    pub fn windows(&self) -> Vec<WindowId> {
        self.windows.clone()
    }

    /// Focus a window (move to front of MRU).
    pub fn focus_window(&mut self, window_id: WindowId) {
        if let Some(pos) = self.windows.iter().position(|&id| id == window_id) {
            self.windows.remove(pos);
            self.windows.insert(0, window_id);
        }
    }

    /// Get the most recently used window on this workspace.
    pub fn mru_window(&self) -> Option<WindowId> {
        self.windows.first().copied()
    }
}

/// Manages all workspaces in the display.
#[derive(Debug)]
pub struct MetaWorkspaceManager {
    /// All workspaces.
    workspaces: Vec<Workspace>,
    /// Index of currently active workspace.
    active_index: usize,
    /// Number of workspaces per row in grid layout (0 = linear).
    workspaces_per_row: usize,
    /// Current grid layout (rows x cols).
    layout_rows: usize,
    layout_cols: usize,
}

impl MetaWorkspaceManager {
    /// Create a new workspace manager with given number of workspaces.
    pub fn new(count: usize) -> Self {
        let mut workspaces = Vec::with_capacity(count);
        for i in 0..count {
            workspaces.push(Workspace::new(
                WorkspaceId(i),
                i,
                alloc::format!("Workspace {}", i + 1),
            ));
        }
        workspaces[0].set_active(true);

        let layout_cols = libm::ceilf(libm::sqrtf(count as f32)) as usize;
        let layout_rows = (count + layout_cols - 1) / layout_cols;

        MetaWorkspaceManager {
            workspaces,
            active_index: 0,
            workspaces_per_row: 0,
            layout_rows,
            layout_cols,
        }
    }

    /// Get total number of workspaces.
    pub fn count(&self) -> usize {
        self.workspaces.len()
    }

    /// Get currently active workspace index.
    pub fn active_index(&self) -> usize {
        self.active_index
    }

    /// Get reference to active workspace.
    pub fn active_workspace(&self) -> &Workspace {
        &self.workspaces[self.active_index]
    }

    /// Get mutable reference to active workspace.
    pub fn active_workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active_index]
    }

    /// Get reference to workspace by index.
    pub fn workspace(&self, index: usize) -> Option<&Workspace> {
        self.workspaces.get(index)
    }

    /// Get mutable reference to workspace by index.
    pub fn workspace_mut(&mut self, index: usize) -> Option<&mut Workspace> {
        self.workspaces.get_mut(index)
    }

    /// Get reference to workspace by ID.
    pub fn workspace_by_id(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|ws| ws.id == id)
    }

    /// Get mutable reference to workspace by ID.
    pub fn workspace_by_id_mut(&mut self, id: WorkspaceId) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|ws| ws.id == id)
    }

    /// Activate a workspace by index.
    pub fn activate_workspace(&mut self, index: usize) -> bool {
        if index >= self.workspaces.len() || index == self.active_index {
            return false;
        }

        self.workspaces[self.active_index].set_active(false);
        self.active_index = index;
        self.workspaces[self.active_index].set_active(true);
        true
    }

    /// Activate a workspace by ID.
    pub fn activate_workspace_by_id(&mut self, id: WorkspaceId) -> bool {
        if let Some(index) = self.workspaces.iter().position(|ws| ws.id == id) {
            self.activate_workspace(index)
        } else {
            false
        }
    }

    /// Switch to the next workspace (wraps around).
    pub fn switch_next(&mut self) -> bool {
        let next_index = (self.active_index + 1) % self.workspaces.len();
        self.activate_workspace(next_index)
    }

    /// Switch to the previous workspace (wraps around).
    pub fn switch_previous(&mut self) -> bool {
        let next_index = if self.active_index == 0 {
            self.workspaces.len() - 1
        } else {
            self.active_index - 1
        };
        self.activate_workspace(next_index)
    }

    /// Add a window to a workspace by index.
    pub fn add_window_to_workspace(&mut self, window_id: WindowId, workspace_index: usize) -> bool {
        if let Some(ws) = self.workspaces.get_mut(workspace_index) {
            ws.add_window(window_id);
            true
        } else {
            false
        }
    }

    /// Remove a window from a workspace by index.
    pub fn remove_window_from_workspace(
        &mut self,
        window_id: WindowId,
        workspace_index: usize,
    ) -> bool {
        if let Some(ws) = self.workspaces.get_mut(workspace_index) {
            ws.remove_window(window_id);
            true
        } else {
            false
        }
    }

    /// Remove a window from all workspaces.
    pub fn remove_window(&mut self, window_id: WindowId) {
        for ws in &mut self.workspaces {
            ws.remove_window(window_id);
        }
    }

    /// Find workspaces containing a window.
    pub fn find_workspaces_with_window(&self, window_id: WindowId) -> Vec<usize> {
        self.workspaces
            .iter()
            .enumerate()
            .filter(|(_, ws)| ws.contains_window(window_id))
            .map(|(i, _)| i)
            .collect()
    }

    /// Move a window to a different workspace.
    pub fn move_window_to_workspace(&mut self, window_id: WindowId, target_index: usize) -> bool {
        if target_index >= self.workspaces.len() {
            return false;
        }
        self.remove_window(window_id);
        self.add_window_to_workspace(window_id, target_index)
    }

    /// Focus a window on its workspace(s).
    pub fn focus_window(&mut self, window_id: WindowId) {
        for ws in &mut self.workspaces {
            if ws.contains_window(window_id) {
                ws.focus_window(window_id);
            }
        }
    }

    /// Get grid layout (rows, cols).
    pub fn layout(&self) -> (usize, usize) {
        (self.layout_rows, self.layout_cols)
    }

    /// Set grid layout.
    pub fn set_layout(&mut self, rows: usize, cols: usize) {
        self.layout_rows = rows;
        self.layout_cols = cols;
    }

    /// Get all active windows across all workspaces.
    pub fn all_windows(&self) -> Vec<WindowId> {
        let mut windows = Vec::new();
        for ws in &self.workspaces {
            for window_id in ws.windows() {
                if !windows.contains(&window_id) {
                    windows.push(window_id);
                }
            }
        }
        windows
    }

    /// Get windows on the active workspace.
    pub fn active_workspace_windows(&self) -> Vec<WindowId> {
        self.workspaces[self.active_index].windows()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_creation() {
        let ws = Workspace::new(WorkspaceId(0), 0, "Test".into());
        assert_eq!(ws.index(), 0);
        assert_eq!(ws.name(), "Test");
        assert_eq!(ws.window_count(), 0);
        assert!(!ws.is_active());
    }

    #[test]
    fn test_manager_creation() {
        let mgr = MetaWorkspaceManager::new(4);
        assert_eq!(mgr.count(), 4);
        assert_eq!(mgr.active_index(), 0);
        assert!(mgr.active_workspace().is_active());
    }

    #[test]
    fn test_window_operations() {
        let mut ws = Workspace::new(WorkspaceId(0), 0, "Workspace".into());
        let window_id = WindowId(1);

        ws.add_window(window_id);
        assert_eq!(ws.window_count(), 1);
        assert!(ws.contains_window(window_id));

        ws.remove_window(window_id);
        assert!(!ws.contains_window(window_id));
    }

    #[test]
    fn test_workspace_switching() {
        let mut mgr = MetaWorkspaceManager::new(4);
        assert_eq!(mgr.active_index(), 0);

        mgr.activate_workspace(2);
        assert_eq!(mgr.active_index(), 2);
        assert!(mgr.active_workspace().is_active());

        mgr.switch_next();
        assert_eq!(mgr.active_index(), 3);

        mgr.switch_next();
        assert_eq!(mgr.active_index(), 0); // Wraps
    }

    #[test]
    fn test_window_movement() {
        let mut mgr = MetaWorkspaceManager::new(2);
        let window_id = WindowId(1);

        mgr.add_window_to_workspace(window_id, 0);
        assert!(mgr.workspace(0).unwrap().contains_window(window_id));

        mgr.move_window_to_workspace(window_id, 1);
        assert!(!mgr.workspace(0).unwrap().contains_window(window_id));
        assert!(mgr.workspace(1).unwrap().contains_window(window_id));
    }

    #[test]
    fn test_mru_order() {
        let mut ws = Workspace::new(WorkspaceId(0), 0, "Test".into());

        ws.add_window(WindowId(1));
        ws.add_window(WindowId(2));
        ws.add_window(WindowId(3));

        assert_eq!(ws.mru_window(), Some(WindowId(3)));

        ws.focus_window(WindowId(1));
        assert_eq!(ws.mru_window(), Some(WindowId(1)));
    }

    #[test]
    fn test_find_window() {
        let mut mgr = MetaWorkspaceManager::new(4);
        let window_id = WindowId(42);

        mgr.add_window_to_workspace(window_id, 0);
        mgr.add_window_to_workspace(window_id, 2);

        let workspaces = mgr.find_workspaces_with_window(window_id);
        assert_eq!(workspaces.len(), 2);
        assert!(workspaces.contains(&0));
        assert!(workspaces.contains(&2));
    }
}
