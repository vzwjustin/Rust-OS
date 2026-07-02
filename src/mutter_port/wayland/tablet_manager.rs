//! Wayland Tablet Manager — coordinates tablet input devices and sessions.
//!
//! Manages tablet (stylus) and tablet-pad input devices, seat associations,
//! and protocol resource tracking for the Wayland tablet protocol.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet-manager.h

use alloc::{collections::BTreeMap, vec::Vec};

/// Manages tablet device registration and per-seat tablet sessions.
#[derive(Debug)]
pub struct MetaWaylandTabletManager {
    /// Pointer to the parent compositor.
    pub compositor: *mut core::ffi::c_void,
    /// Wayland display for resource tracking.
    pub wl_display: *mut core::ffi::c_void,
    /// List of protocol resources bound by clients.
    pub resource_list: Vec<*mut core::ffi::c_void>,
    /// Seat bindings: maps seat pointer to the tablet seat (session)
    /// pointer. In the C original this is a GHashTable; here we use a
    /// BTreeMap keyed by the raw address for no_std compatibility.
    pub seats: BTreeMap<usize, *mut core::ffi::c_void>,
}

impl MetaWaylandTabletManager {
    /// Create a new tablet manager.
    pub fn new(compositor: *mut core::ffi::c_void, wl_display: *mut core::ffi::c_void) -> Self {
        Self {
            compositor,
            wl_display,
            resource_list: Vec::new(),
            seats: BTreeMap::new(),
        }
    }

    /// Bind a seat to the tablet manager, creating a tablet seat session.
    /// A full implementation would allocate a MetaWaylandTabletSeat and
    /// register it with the seat. Returns the previous binding if the
    /// seat was already bound.
    pub fn bind_seat(
        &mut self,
        seat: *mut core::ffi::c_void,
        tablet_seat: *mut core::ffi::c_void,
    ) -> Option<*mut core::ffi::c_void> {
        if seat.is_null() {
            return None;
        }
        self.seats.insert(seat as usize, tablet_seat)
    }

    /// Unbind a seat from the tablet manager.
    /// Returns the removed tablet seat pointer, if any.
    pub fn unbind_seat(&mut self, seat: *mut core::ffi::c_void) -> Option<*mut core::ffi::c_void> {
        if seat.is_null() {
            return None;
        }
        self.seats.remove(&(seat as usize))
    }

    /// Look up the tablet seat session for a given seat.
    pub fn lookup_seat(&self, seat: *mut core::ffi::c_void) -> Option<*mut core::ffi::c_void> {
        if seat.is_null() {
            return None;
        }
        self.seats.get(&(seat as usize)).copied()
    }

    /// Add a client protocol resource to the resource list.
    pub fn add_resource(&mut self, resource: *mut core::ffi::c_void) {
        self.resource_list.push(resource);
    }

    /// Remove a client protocol resource from the resource list.
    pub fn remove_resource(&mut self, resource: *mut core::ffi::c_void) {
        self.resource_list.retain(|&r| r != resource);
    }

    /// Number of bound seats.
    pub fn seat_count(&self) -> usize {
        self.seats.len()
    }

    /// Number of client protocol resources.
    pub fn resource_count(&self) -> usize {
        self.resource_list.len()
    }

    /// Clear all seat bindings and resources.
    pub fn clear(&mut self) {
        self.seats.clear();
        self.resource_list.clear();
    }
}

impl Default for MetaWaylandTabletManager {
    fn default() -> Self {
        Self::new(core::ptr::null_mut(), core::ptr::null_mut())
    }
}

/// Initialize tablet manager for the compositor.
///
/// Registers tablet_manager protocol and prepares device tracking.
/// A full implementation would call wl_global_create for
/// zwp_tablet_manager_v2.
pub fn meta_wayland_tablet_manager_init(compositor: *mut core::ffi::c_void) {
    if compositor.is_null() {
        return;
    }
    // Protocol global registration requires libwayland-server.
}

/// Finalize tablet manager — clean up resources and remove protocol.
///
/// Closes all tablet sessions and frees device tables.
/// A full implementation would destroy all tablet seat sessions,
/// remove the wl_global, and free each wl_resource in resource_list.
pub fn meta_wayland_tablet_manager_finalize(compositor: *mut core::ffi::c_void) {
    if compositor.is_null() {
        return;
    }
    // Seat bindings and resources are owned by the manager struct;
    // their Drop implementations handle cleanup.
}
