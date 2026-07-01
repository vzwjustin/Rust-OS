//! Wayland Data Source Primary module
//!
//! Primary selection data source (X11 primary clipboard equivalent).
//! Inherits from MetaWaylandDataSource for protocol compatibility.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-data-source-primary.h

/// Primary selection data source. Opaque handle to base MetaWaylandDataSource.
pub struct MetaWaylandDataSourcePrimary;

impl MetaWaylandDataSourcePrimary {
    /// Create a new primary data source from wl_resource.
    /// Returns pointer to MetaWaylandDataSource base.
    pub fn new(_resource: *mut core::ffi::c_void) -> *mut core::ffi::c_void {
        // TODO: implement - allocate and initialize base MetaWaylandDataSource
        core::ptr::null_mut()
    }
}
