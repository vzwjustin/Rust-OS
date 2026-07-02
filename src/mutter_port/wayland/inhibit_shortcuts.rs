//! Wayland Inhibit Shortcuts module
//!
//! Manages keyboard shortcuts inhibition. Allows clients to temporarily disable
//! compositor shortcuts for fullscreen games and applications.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-inhibit-shortcuts.h

use alloc::vec::Vec;

/// Keyboard shortcuts inhibit manager.
///
/// Tracks the set of Wayland surfaces that have requested keyboard
/// shortcut inhibition via the `zwp_keyboard_shortcuts_inhibit_manager_v1`
/// protocol. While a surface is in the inhibited set, the compositor must
/// not intercept shortcuts (e.g. Alt+Tab, Super) for that surface's
/// keyboard focus.
pub struct MetaWaylandKeyboardShortcutsInhibit {
    /// Surfaces with an active shortcuts inhibitor.
    pub inhibited_surfaces: Vec<InhibitEntry>,
}

/// A single inhibit entry pairing a surface with its inhibitor resource.
#[derive(Debug, Clone, Copy)]
pub struct InhibitEntry {
    /// MetaWaylandSurface pointer.
    pub surface: *mut core::ffi::c_void,
    /// zwp_keyboard_shortcuts_inhibitor_v1 resource pointer.
    pub inhibitor: *mut core::ffi::c_void,
}

impl MetaWaylandKeyboardShortcutsInhibit {
    /// Create a new shortcuts inhibit manager with no inhibitors.
    pub fn new() -> Self {
        Self {
            inhibited_surfaces: Vec::new(),
        }
    }

    /// Initialize keyboard shortcuts inhibit protocol support for the
    /// compositor.
    ///
    /// A full implementation would call `wl_global_create` to advertise
    /// the `zwp_keyboard_shortcuts_inhibit_manager_v1` global and register
    /// a bind handler that dispenses `zwp_keyboard_shortcuts_inhibitor_v1`
    /// resources. Without libwayland, this returns true to indicate the
    /// manager state is ready to track inhibitors.
    pub fn init(_compositor: *mut core::ffi::c_void) -> bool {
        // With libwayland: wl_global_create(compositor->wl_display,
        //   &zwp_keyboard_shortcuts_inhibit_manager_v1_interface, 1,
        //   compositor, bind_inhibit_shortcuts). The bind handler exposes
        //   the `inhibit_shortcuts` request which creates a per-surface
        //   inhibitor resource; the resource destroy handler calls
        //   remove_inhibitor below.
        true
    }

    /// Register a shortcuts inhibitor for a surface. Called when a client
    /// sends the `inhibit_shortcuts` request on the manager. Replaces any
    /// existing inhibitor for the same surface.
    pub fn add_inhibitor(
        &mut self,
        surface: *mut core::ffi::c_void,
        inhibitor: *mut core::ffi::c_void,
    ) {
        if surface.is_null() {
            return;
        }
        if let Some(entry) = self
            .inhibited_surfaces
            .iter_mut()
            .find(|e| core::ptr::eq(e.surface, surface))
        {
            entry.inhibitor = inhibitor;
        } else {
            self.inhibited_surfaces
                .push(InhibitEntry { surface, inhibitor });
        }
    }

    /// Remove the inhibitor for a surface (e.g. when the client destroys
    /// the inhibitor resource or the surface is destroyed). Returns true
    /// if an inhibitor was present and removed.
    pub fn remove_inhibitor(&mut self, surface: *mut core::ffi::c_void) -> bool {
        let before = self.inhibited_surfaces.len();
        self.inhibited_surfaces
            .retain(|e| !core::ptr::eq(e.surface, surface));
        self.inhibited_surfaces.len() != before
    }

    /// Whether a given surface currently has shortcuts inhibited.
    pub fn is_inhibited(&self, surface: *mut core::ffi::c_void) -> bool {
        self.inhibited_surfaces
            .iter()
            .any(|e| core::ptr::eq(e.surface, surface))
    }

    /// Look up the inhibitor resource for a surface, if any.
    pub fn lookup_inhibitor(
        &self,
        surface: *mut core::ffi::c_void,
    ) -> Option<*mut core::ffi::c_void> {
        self.inhibited_surfaces
            .iter()
            .find(|e| core::ptr::eq(e.surface, surface))
            .map(|e| e.inhibitor)
    }

    /// Number of surfaces with active shortcut inhibition.
    pub fn inhibited_count(&self) -> usize {
        self.inhibited_surfaces.len()
    }
}

impl Default for MetaWaylandKeyboardShortcutsInhibit {
    fn default() -> Self {
        Self::new()
    }
}
