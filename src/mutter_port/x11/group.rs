//! X11 window group management.
//!
//! Ported from GNOME Mutter's src/x11/meta-x11-group.c/.h.
//! Groups related windows together (ICCCM window groups) for shared operations
//! like minimize, focus, and modal dialogs.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-x11-group.c

use crate::mutter_port::x11::display::XWindow;
use alloc::vec::Vec;

/// Opaque window group handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(pub u64);

/// Represents an ICCCM window group (e.g., all windows from one application).
pub struct MetaX11Group {
    pub group_id: GroupId,
    pub leader_window: XWindow,

    /// Windows in this group.
    pub windows: Vec<u64>, // MetaWindow handles

    /// Group properties.
    pub startup_id: Option<alloc::string::String>,
    pub wm_client_machine: Option<alloc::string::String>,
}

impl MetaX11Group {
    /// Create a new window group with a leader window.
    /// # TODO: port logic from meta_x11_group_new()
    pub fn new(group_id: GroupId, leader_window: XWindow) -> Self {
        Self {
            group_id,
            leader_window,
            windows: Vec::new(),
            startup_id: None,
            wm_client_machine: None,
        }
    }

    /// Add a window to this group.
    /// # TODO: port logic from meta_x11_group_add_window()
    pub fn add_window(&mut self, meta_window_id: u64) {
        if !self.windows.contains(&meta_window_id) {
            self.windows.push(meta_window_id);
        }
    }

    /// Remove a window from this group.
    /// # TODO: port logic from meta_x11_group_remove_window()
    pub fn remove_window(&mut self, meta_window_id: u64) {
        self.windows.retain(|&id| id != meta_window_id);
    }

    /// Get all windows in this group.
    pub fn get_windows(&self) -> &[u64] {
        &self.windows
    }

    /// Check if this group is empty.
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Set the startup ID for this group.
    /// # TODO: port logic from group startup notification handling
    pub fn set_startup_id(&mut self, startup_id: alloc::string::String) {
        self.startup_id = Some(startup_id);
    }

    /// Set the WM client machine for this group.
    /// # TODO: port logic from meta_x11_group_set_wm_client_machine()
    pub fn set_wm_client_machine(&mut self, machine: alloc::string::String) {
        self.wm_client_machine = Some(machine);
    }

    /// Update group properties from group leader window.
    /// # TODO: port logic from meta_x11_group_update_window()
    pub fn update_properties(&mut self) {
        // TODO: read group properties from leader window
    }
}
