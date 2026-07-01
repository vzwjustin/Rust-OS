//! Wayland Filter Manager protocol implementation.
//!
//! Ported from: meta-wayland-filter-manager.c/h
//!
//! Implements filtering of Wayland globals (protocol objects) on a per-client basis.
//! Used to restrict which protocols are exposed to which clients based on
//! security policies or client properties.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-filter-manager.h

use alloc::vec::Vec;

/// Filter result for a global resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaWaylandAccess {
    /// Global is allowed for this client.
    ALLOWED = 0,
    /// Global is denied for this client.
    DENIED = 1,
}

/// Callback function type for filtering a global.
///
/// Called with the filter manager, global, and user data to determine
/// if a global should be exposed to a client. Returns a MetaWaylandAccess value.
pub type MetaWaylandFilterFunc = Option<
    unsafe extern "C" fn(
        *const core::ffi::c_void,
        *const core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> u32,
>;

/// Encapsulates a filter rule (global + callback + user data).
#[derive(Debug)]
pub struct FilterEntry {
    pub global: *mut core::ffi::c_void,
    pub filter_func: MetaWaylandFilterFunc,
    pub user_data: *mut core::ffi::c_void,
}

/// Filter manager for Wayland compositor globals.
///
/// Maintains a list of global resource filters. Protocol I/O is TODO;
/// this holds the manager state and filter list.
#[derive(Debug)]
pub struct MetaWaylandFilterManager {
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
    pub filters: Vec<FilterEntry>,
}

impl MetaWaylandFilterManager {
    pub fn new(compositor: *mut core::ffi::c_void) -> Self {
        MetaWaylandFilterManager {
            compositor: if compositor.is_null() { None } else { Some(compositor) },
            filters: Vec::new(),
        }
    }

    /// Add a filter for a global resource.
    pub fn add_filter(
        &mut self,
        global: *mut core::ffi::c_void,
        filter_func: MetaWaylandFilterFunc,
        user_data: *mut core::ffi::c_void,
    ) {
        self.filters.push(FilterEntry {
            global,
            filter_func,
            user_data,
        });
    }

    /// Remove all filters for a given global.
    pub fn remove_filters_for_global(&mut self, global: *mut core::ffi::c_void) {
        self.filters.retain(|entry| entry.global != global);
    }
}

impl Default for MetaWaylandFilterManager {
    fn default() -> Self {
        MetaWaylandFilterManager {
            compositor: None,
            filters: Vec::new(),
        }
    }
}
