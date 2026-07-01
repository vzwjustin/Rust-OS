//! Mutter manager types for various subsystems
//! Ported from meta/meta-*-manager.h
//!
//! Managers coordinate device input, orientation, workspaces, and system state.
use alloc::{string::String, vec::Vec, boxed::Box};

use crate::mutter_port::meta::types::*;
// Use the rich workspace type (types::* only provides an opaque stub); this
// matches what `meta::MetaWorkspace` re-exports.
use crate::mutter_port::meta::workspace::MetaWorkspace;

/// Manages idle detection and timeouts
pub struct MetaIdleMonitor {
    idle_time_ms: u32,
    watches: Vec<u32>,
}

impl MetaIdleMonitor {
    /// Create a new MetaIdleMonitor
    pub fn new() -> Self {
        Self {
            idle_time_ms: 0,
            watches: Vec::new(),
        }
    }

    /// Get idle time in milliseconds
    pub fn get_idle_time(&self) -> u32 {
        self.idle_time_ms
    }

    /// Add idle watch callback
    pub fn add_watch(&mut self, _timeout_ms: u32) {
        // TODO: implement
    }

    /// Remove idle watch
    pub fn remove_watch(&mut self, _watch_id: u32) {
        // TODO: implement
    }

    /// Reset idle timer
    pub fn reset(&mut self) {
        self.idle_time_ms = 0;
        // TODO: implement
    }
}

impl Default for MetaIdleMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages screen orientation/rotation
pub struct MetaOrientationManager {
    orientation: MetaOrientation,
    has_lock: bool,
}

impl MetaOrientationManager {
    /// Create a new MetaOrientationManager
    pub fn new() -> Self {
        Self {
            orientation: MetaOrientation::Normal,
            has_lock: false,
        }
    }

    /// Get current screen orientation
    pub fn get_orientation(&self) -> MetaOrientation {
        self.orientation
    }

    /// Set screen orientation
    pub fn set_orientation(&mut self, orientation: MetaOrientation) {
        self.orientation = orientation;
        // TODO: implement
    }

    /// Check if orientation auto-rotation is enabled
    pub fn has_orientation_lock(&self) -> bool {
        self.has_lock
    }
}

impl Default for MetaOrientationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Screen orientation values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaOrientation {
    Normal = 0,
    Rotated90 = 1,
    Rotated180 = 2,
    Rotated270 = 3,
}

/// Manages workspace switching and properties
pub struct MetaWorkspaceManager {
    display: Option<Box<MetaDisplay>>,
    workspaces: Vec<Box<MetaWorkspace>>,
    active_index: u32,
}

impl MetaWorkspaceManager {
    /// Create a new MetaWorkspaceManager
    pub fn new() -> Self {
        Self {
            display: None,
            workspaces: Vec::new(),
            active_index: 0,
        }
    }

    /// Get the display this manager belongs to
    pub fn get_display(&self) -> Option<&MetaDisplay> {
        self.display.as_ref().map(|b| &**b)
    }

    /// Get workspace count
    pub fn get_n_workspaces(&self) -> u32 {
        self.workspaces.len() as u32
    }

    /// Get workspace by index
    pub fn get_workspace_by_index(&self, index: u32) -> Option<&MetaWorkspace> {
        self.workspaces.get(index as usize).map(|w| w.as_ref())
    }

    /// Get active workspace
    pub fn get_active_workspace(&self) -> Option<&MetaWorkspace> {
        self.workspaces
            .get(self.active_index as usize)
            .map(|w| w.as_ref())
    }

    /// Create a new workspace, appended at the end, and return its index.
    pub fn create_workspace(&mut self, name: Option<&str>) -> u32 {
        let index = self.workspaces.len() as u32;
        let mut ws = MetaWorkspace::new(index);
        if let Some(n) = name {
            ws.set_name(Some(String::from(n)));
        }
        self.workspaces.push(Box::new(ws));
        index
    }

    /// Remove the given workspace (matched by identity). Clamps the active
    /// index if it now points past the end.
    pub fn remove_workspace(&mut self, workspace: &MetaWorkspace) {
        if let Some(pos) = self
            .workspaces
            .iter()
            .position(|w| core::ptr::eq(w.as_ref(), workspace))
        {
            self.workspaces.remove(pos);
            if self.active_index as usize >= self.workspaces.len() {
                self.active_index = (self.workspaces.len().saturating_sub(1)) as u32;
            }
        }
    }

    /// Reorder workspaces
    pub fn reorder_workspace(&mut self, _from: u32, _to: u32) {
        // TODO: implement
    }
}

impl Default for MetaWorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Debug and development control
pub struct MetaDebugControl {
    is_enabled: bool,
    debug_log: Vec<String>,
}

impl MetaDebugControl {
    /// Create a new MetaDebugControl
    pub fn new() -> Self {
        Self {
            is_enabled: false,
            debug_log: Vec::new(),
        }
    }

    /// Enable debug mode
    pub fn set_debug_mode(&mut self, enabled: bool) {
        self.is_enabled = enabled;
        // TODO: implement
    }

    /// Get debug status
    pub fn is_debug_enabled(&self) -> bool {
        self.is_enabled
    }

    /// Get debug log
    pub fn get_debug_log(&self) -> Option<Vec<String>> {
        if self.debug_log.is_empty() {
            None
        } else {
            Some(self.debug_log.clone())
        }
    }
}

impl Default for MetaDebugControl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_index_workspaces() {
        let mut m = MetaWorkspaceManager::new();
        assert_eq!(m.get_n_workspaces(), 0);
        assert!(m.get_active_workspace().is_none());

        assert_eq!(m.create_workspace(Some("one")), 0);
        assert_eq!(m.create_workspace(Some("two")), 1);
        assert_eq!(m.get_n_workspaces(), 2);

        assert_eq!(m.get_workspace_by_index(0).and_then(|w| w.get_name()), Some("one"));
        assert_eq!(m.get_workspace_by_index(1).and_then(|w| w.get_name()), Some("two"));
        assert!(m.get_workspace_by_index(2).is_none());
        // active_index defaults to 0.
        assert_eq!(m.get_active_workspace().and_then(|w| w.get_name()), Some("one"));
    }

    #[test]
    fn test_remove_non_member_is_noop() {
        let mut m = MetaWorkspaceManager::new();
        m.create_workspace(Some("a"));
        m.create_workspace(Some("b"));
        // A workspace the manager doesn't own must not be removed.
        let stray = MetaWorkspace::new(99);
        m.remove_workspace(&stray);
        assert_eq!(m.get_n_workspaces(), 2);
    }
}
