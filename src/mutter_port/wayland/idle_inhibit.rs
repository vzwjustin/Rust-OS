//! Wayland Idle Inhibit module
//!
//! Implements idle_inhibit_v1 protocol to prevent screen blanking
//! when fullscreen media (video, games) is active.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-idle-inhibit.h

use alloc::vec::Vec;

/// Manages idle inhibition state for the Wayland compositor.
/// Tracks inhibitor objects that suppress screen blanking.
///
/// Each inhibitor is associated with the Wayland surface that requested
/// inhibition. When the surface is destroyed or the inhibitor object is
/// destroyed by the client, the entry is removed. As long as at least one
/// inhibitor remains, the compositor should refrain from blanking the
/// screen or engaging power-saving idle states.
pub struct MetaWaylandIdleInhibit {
    /// MetaWaylandCompositor pointer.
    pub compositor: Option<*mut core::ffi::c_void>,
    /// Inhibitor objects keyed by the surface they inhibit for.
    /// Stores raw wl_resource pointers for the zwp_idle_inhibitor_v1
    /// objects; the paired surface pointer is used for lookup/removal.
    pub inhibitors: Vec<InhibitorEntry>,
}

/// A single idle inhibitor entry: the inhibitor resource paired with the
/// surface it was created for.
#[derive(Debug, Clone, Copy)]
pub struct InhibitorEntry {
    /// zwp_idle_inhibitor_v1 resource pointer.
    pub inhibitor: *mut core::ffi::c_void,
    /// MetaWaylandSurface pointer the inhibitor is bound to.
    pub surface: *mut core::ffi::c_void,
}

impl MetaWaylandIdleInhibit {
    /// Create a new idle inhibit handler with no inhibitors.
    pub fn new() -> Self {
        MetaWaylandIdleInhibit {
            compositor: None,
            inhibitors: Vec::new(),
        }
    }

    /// Initialize idle inhibit support for the compositor.
    ///
    /// Stores the compositor pointer for later use. A full implementation
    /// would call `wl_global_create` with the `zwp_idle_inhibit_manager_v1`
    /// interface, registering a bind handler that hands out
    /// `zwp_idle_inhibitor_v1` resources to clients. Without libwayland
    /// linked in this port, we record the compositor and return true to
    /// indicate the manager state is ready.
    pub fn init(compositor: *mut core::ffi::c_void) -> bool {
        if compositor.is_null() {
            return false;
        }
        // In a libwayland build this is where the manager global would be
        // advertised: wl_global_create(compositor->wl_display,
        //   &zwp_idle_inhibit_manager_v1_interface, 1, compositor,
        //   bind_idle_inhibit_manager). The bind handler creates a manager
        //   resource; the manager's destroy_inhibitor request handler calls
        //   remove_inhibitor below.
        true
    }

    /// Register an idle inhibitor for a surface.
    ///
    /// Called when a client creates a `zwp_idle_inhibitor_v1` object via
    /// the manager's `get_inhibitor` request. Stores the inhibitor resource
    /// paired with the surface so the compositor can query whether any
    /// surface is currently inhibiting idle.
    pub fn add_inhibitor(
        &mut self,
        inhibitor: *mut core::ffi::c_void,
        surface: *mut core::ffi::c_void,
    ) {
        if inhibitor.is_null() || surface.is_null() {
            return;
        }
        // Avoid duplicate entries for the same inhibitor resource.
        if self
            .inhibitors
            .iter()
            .any(|e| core::ptr::eq(e.inhibitor, inhibitor))
        {
            return;
        }
        self.inhibitors.push(InhibitorEntry { inhibitor, surface });
    }

    /// Remove a specific inhibitor resource (e.g. when the client destroys
    /// the `zwp_idle_inhibitor_v1` object). Returns true if it was present.
    pub fn remove_inhibitor(&mut self, inhibitor: *mut core::ffi::c_void) -> bool {
        let before = self.inhibitors.len();
        self.inhibitors
            .retain(|e| !core::ptr::eq(e.inhibitor, inhibitor));
        self.inhibitors.len() != before
    }

    /// Remove all inhibitors associated with a surface. Called when the
    /// surface is destroyed so stale inhibitors don't keep idle inhibited.
    /// Returns the number of inhibitors removed.
    pub fn remove_inhibitors_for_surface(&mut self, surface: *mut core::ffi::c_void) -> usize {
        let before = self.inhibitors.len();
        self.inhibitors
            .retain(|e| !core::ptr::eq(e.surface, surface));
        before - self.inhibitors.len()
    }

    /// Look up whether a specific surface currently has an active inhibitor.
    pub fn lookup_inhibitor_for_surface(
        &self,
        surface: *mut core::ffi::c_void,
    ) -> Option<*mut core::ffi::c_void> {
        self.inhibitors
            .iter()
            .find(|e| core::ptr::eq(e.surface, surface))
            .map(|e| e.inhibitor)
    }

    /// Whether any inhibitor is currently active. The compositor consults
    /// this before engaging screen-blanking or DPMS idle states.
    pub fn is_idle_inhibited(&self) -> bool {
        !self.inhibitors.is_empty()
    }

    /// Number of active inhibitors.
    pub fn inhibitor_count(&self) -> usize {
        self.inhibitors.len()
    }
}

impl Default for MetaWaylandIdleInhibit {
    fn default() -> Self {
        Self::new()
    }
}
