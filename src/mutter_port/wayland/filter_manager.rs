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
/// Maintains a list of global resource filters. A full implementation
/// would hook into `wl_global` filter dispatch (libwayland's
/// `wl_display_set_global_filter`) so that each client only sees the
/// globals whose filter callbacks return `MetaWaylandAccess::ALLOWED`.
/// Without libwayland linked in this port, the manager holds the filter
/// list and exposes query/eval helpers that the compositor can consult
/// before advertising a global to a client.
#[derive(Debug)]
pub struct MetaWaylandFilterManager {
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
    pub filters: Vec<FilterEntry>,
}

impl MetaWaylandFilterManager {
    pub fn new(compositor: *mut core::ffi::c_void) -> Self {
        MetaWaylandFilterManager {
            compositor: if compositor.is_null() {
                None
            } else {
                Some(compositor)
            },
            filters: Vec::new(),
        }
    }

    /// Add a filter for a global resource. If a filter for the same
    /// global already exists, it is replaced (mirrors the C code which
    /// keeps at most one filter per global).
    pub fn add_filter(
        &mut self,
        global: *mut core::ffi::c_void,
        filter_func: MetaWaylandFilterFunc,
        user_data: *mut core::ffi::c_void,
    ) {
        if let Some(entry) = self
            .filters
            .iter_mut()
            .find(|e| core::ptr::eq(e.global, global))
        {
            entry.filter_func = filter_func;
            entry.user_data = user_data;
            return;
        }
        self.filters.push(FilterEntry {
            global,
            filter_func,
            user_data,
        });
    }

    /// Remove all filters for a given global. Returns the number removed.
    pub fn remove_filters_for_global(&mut self, global: *mut core::ffi::c_void) -> usize {
        let before = self.filters.len();
        self.filters
            .retain(|entry| !core::ptr::eq(entry.global, global));
        before - self.filters.len()
    }

    /// Get the filter entry for a global, if any.
    pub fn get_filter(&self, global: *mut core::ffi::c_void) -> Option<&FilterEntry> {
        self.filters
            .iter()
            .find(|e| core::ptr::eq(e.global, global))
    }

    /// Number of registered filters.
    pub fn filter_count(&self) -> usize {
        self.filters.len()
    }

    /// Evaluate the access decision for a global/client pair by invoking
    /// the registered filter callback. If no filter is registered for the
    /// global, the global is allowed by default (matching libwayland's
    /// behaviour where an unfiltered global is visible to all clients).
    ///
    /// # Safety
    /// Calls a foreign function pointer with the provided client and
    /// global pointers; the caller must ensure both are valid for the
    /// duration of the callback.
    pub unsafe fn evaluate(
        &self,
        global: *const core::ffi::c_void,
        client: *const core::ffi::c_void,
    ) -> MetaWaylandAccess {
        match self.get_filter(global as *mut _) {
            Some(entry) => match entry.filter_func {
                Some(f) => {
                    let result = f(global, client, entry.user_data);
                    if result == MetaWaylandAccess::DENIED as u32 {
                        MetaWaylandAccess::DENIED
                    } else {
                        MetaWaylandAccess::ALLOWED
                    }
                }
                None => MetaWaylandAccess::ALLOWED,
            },
            None => MetaWaylandAccess::ALLOWED,
        }
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
