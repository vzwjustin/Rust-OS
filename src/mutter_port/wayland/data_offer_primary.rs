//! Wayland Data Offer Primary module
//!
//! Ported from: meta-wayland-data-offer-primary.c/h

use alloc::{string::String, vec::Vec, format};

pub struct MetaWaylandDataOfferPrimary {
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
    pub resource: Option<*mut core::ffi::c_void>,   // wl_resource pointer
}

impl MetaWaylandDataOfferPrimary {
    /// Create a new primary data offer
    /// TODO: port logic from meta_wayland_data_offer_primary_new
    pub fn new(
        _compositor: *mut core::ffi::c_void,
        _target: *mut core::ffi::c_void,
    ) -> Option<*mut core::ffi::c_void> {
        // TODO: implement - returns MetaWaylandDataOffer
        None
    }
}
