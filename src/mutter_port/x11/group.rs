//! X11 window group management.
//!
//! Ported from GNOME Mutter's src/x11/meta-x11-group.c/.h.
//! Groups related windows together (ICCCM window groups) for shared operations
//! like minimize, focus, and modal dialogs.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-x11-group.c

use crate::mutter_port::x11::display::XWindow;
use alloc::string::String;
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
    pub startup_id: Option<String>,
    pub wm_client_machine: Option<String>,

    /// Whether the group properties have been read from the leader window.
    pub properties_loaded: bool,
}

impl MetaX11Group {
    /// Create a new window group with a leader window.
    pub fn new(group_id: GroupId, leader_window: XWindow) -> Self {
        Self {
            group_id,
            leader_window,
            windows: Vec::new(),
            startup_id: None,
            wm_client_machine: None,
            properties_loaded: false,
        }
    }

    /// Add a window to this group.
    pub fn add_window(&mut self, meta_window_id: u64) {
        if !self.windows.contains(&meta_window_id) {
            self.windows.push(meta_window_id);
        }
    }

    /// Remove a window from this group.
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
    pub fn set_startup_id(&mut self, startup_id: String) {
        self.startup_id = Some(startup_id);
    }

    /// Set the WM client machine for this group.
    pub fn set_wm_client_machine(&mut self, machine: String) {
        self.wm_client_machine = Some(machine);
    }

    /// Update group properties from the group leader window.
    ///
    /// A full implementation would read the WM_CLIENT_LEADER, WM_CLIENT_MACHINE
    /// and _NET_STARTUP_ID properties from the leader window via
    /// XGetWindowProperty. Without an X connection we record that the
    /// properties have been requested and mark the group as loaded so callers
    /// can distinguish a group whose properties were fetched from one that has
    /// not yet been queried. Callers populate `startup_id` / `wm_client_machine`
    /// via the setters once the backend returns the property values.
    pub fn update_properties(&mut self) {
        // A full implementation would, for the leader window:
        //  1. Read WM_CLIENT_MACHINE (TEXT) -> set wm_client_machine.
        //  2. Read _NET_STARTUP_ID (UTF8_STRING) -> set startup_id.
        //  3. Read WM_CLIENT_LEADER (WINDOW) to confirm the group leader.
        // These XGetWindowProperty calls require the opaque Display* handle
        // owned by the platform backend, which delivers the values through the
        // setters above. Here we mark the group as having been queried so the
        // caller can avoid repeated property reads.
        self.properties_loaded = true;
    }

    /// Returns true once `update_properties` has been called for this group.
    pub fn properties_loaded(&self) -> bool {
        self.properties_loaded
    }

    /// Number of windows currently in the group.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }
}
