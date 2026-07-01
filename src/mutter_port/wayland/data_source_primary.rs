//! Wayland Data Source Primary module
//!
//! Ported from: meta-wayland-data-source-primary.c/h

use alloc::{string::String, vec::Vec, format};

pub struct MetaWaylandDataSourcePrimary {
    pub resource: Option<*mut core::ffi::c_void>, // wl_resource pointer
}

impl MetaWaylandDataSourcePrimary {
    /// Create a new primary data source
    /// TODO: port logic from meta_wayland_data_source_primary_new
    pub fn new(_resource: *mut core::ffi::c_void) -> Option<*mut core::ffi::c_void> {
        // TODO: implement - returns MetaWaylandDataSource
        None
    }
}
