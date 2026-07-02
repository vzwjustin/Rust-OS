//! Wayland XDG Toplevel Tag module
//!
//! Window tagging extension for xdg_shell. Allows applications to tag windows
//! for grouping and lifecycle management.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-xdg-toplevel-tag.h

use alloc::string::String;

/// XDG toplevel tag protocol manager.
///
/// In the C original, `MetaWaylandXdgToplevelTag` wraps the
/// zxdg_toplevel_tag_v1 resource and stores the tag string set by the
/// client. The tag is an opaque string that the compositor can use for
/// window grouping, session restoration, or lifecycle management.
#[derive(Debug)]
pub struct MetaWaylandXdgToplevelTag {
    /// The tag string set by the client, or empty if unset.
    pub tag: String,
    /// wl_resource pointer for the zxdg_toplevel_tag_v1 object.
    pub resource: *mut core::ffi::c_void,
}

impl MetaWaylandXdgToplevelTag {
    /// Create a new toplevel tag with an empty tag string.
    pub fn new() -> Self {
        MetaWaylandXdgToplevelTag {
            tag: String::new(),
            resource: core::ptr::null_mut(),
        }
    }

    /// Create a new toplevel tag bound to a wl_resource.
    pub fn new_with_resource(resource: *mut core::ffi::c_void) -> Self {
        MetaWaylandXdgToplevelTag {
            tag: String::new(),
            resource,
        }
    }

    /// Set the tag string for this toplevel.
    /// A full implementation would validate the tag length and emit
    /// the tag event to confirm the tag was accepted.
    pub fn set_tag(&mut self, tag: String) {
        self.tag = tag;
    }

    /// Get the tag string for this toplevel.
    pub fn get_tag(&self) -> &str {
        &self.tag
    }

    /// Check whether a tag has been set.
    pub fn has_tag(&self) -> bool {
        !self.tag.is_empty()
    }

    /// Clear the tag string.
    pub fn clear_tag(&mut self) {
        self.tag.clear();
    }

    /// Get the wl_resource pointer.
    pub fn get_resource(&self) -> *mut core::ffi::c_void {
        self.resource
    }

    /// Set the wl_resource pointer.
    pub fn set_resource(&mut self, resource: *mut core::ffi::c_void) {
        self.resource = resource;
    }

    /// Initialize XDG toplevel tag protocol support for the compositor.
    /// A full implementation would register the zxdg_toplevel_tag_v1
    /// global via wl_global_create.
    pub fn init(_compositor: *mut core::ffi::c_void) {
        // Protocol global registration requires libwayland-server.
    }
}

impl Default for MetaWaylandXdgToplevelTag {
    fn default() -> Self {
        Self::new()
    }
}
