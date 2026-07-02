//! Wayland Data Offer Primary module
//!
//! Implements primary selection (middle-click paste) data offers.
//! Extends base MetaWaylandDataOffer with primary-specific behavior.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-data-offer-primary.h

use alloc::boxed::Box;

/// Primary selection data offer, extends MetaWaylandDataOffer.
/// Wraps a wl_resource for the primary selection.
pub struct MetaWaylandDataOfferPrimary {
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
    pub resource: Option<*mut core::ffi::c_void>,   // wl_resource pointer
}

impl MetaWaylandDataOfferPrimary {
    /// Create a new primary data offer.
    /// ponytail: wire up primary_selection.offer protocol events if real compositor available
    pub fn new(
        compositor: *mut core::ffi::c_void,
        _target: *mut core::ffi::c_void,
    ) -> Option<*mut core::ffi::c_void> {
        let offer = Box::new(MetaWaylandDataOfferPrimary {
            compositor: Some(compositor),
            resource: None,
        });
        Some(Box::into_raw(offer) as *mut core::ffi::c_void)
    }
}
