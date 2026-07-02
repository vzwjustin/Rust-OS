//! Wayland Selection Source module
//!
//! Adapts a Wayland data source to the MetaSelectionSource interface.
//! Provides MIME type detection and async read capability for clipboard operations.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-selection-source-wayland.c

use alloc::{string::String, vec::Vec};

/// Selection source wrapping a Wayland data source.
/// Inherits from MetaSelectionSource for compositor clipboard abstraction.
pub struct MetaSelectionSourceWayland {
    /// Base MetaWaylandDataSource pointer.
    pub data_source: *mut core::ffi::c_void,
    /// Cached MIME type list for this source, extracted from the
    /// underlying wl_data_source. Each entry is an owned String so
    /// the list is self-contained and does not depend on the C-side
    /// GList lifetime.
    pub mimetypes: Vec<String>,
}

impl MetaSelectionSourceWayland {
    /// Create a new selection source from a Wayland data source.
    ///
    /// In the C original, `meta_selection_source_wayland_new` allocates a
    /// GObject, wraps the `MetaWaylandDataSource`, and calls
    /// `meta_wayland_data_source_get_mime_types` to extract the MIME type
    /// list from the underlying `wl_data_source`. Without libwayland we
    /// cannot introspect the data source, so the MIME list is left empty
    /// and can be populated via `set_mimetypes` when the compositor backend
    /// provides the list.
    pub fn new(source: *mut core::ffi::c_void) -> Self {
        MetaSelectionSourceWayland {
            data_source: source,
            mimetypes: Vec::new(),
        }
    }

    /// Create a new selection source with an explicit MIME type list.
    /// This is the path used when the compositor has already queried the
    /// data source's MIME types (e.g. via wl_data_offer.source_mime_types).
    pub fn new_with_mimetypes(source: *mut core::ffi::c_void, mimetypes: Vec<String>) -> Self {
        MetaSelectionSourceWayland {
            data_source: source,
            mimetypes,
        }
    }

    /// Set the MIME type list for this source, replacing any existing list.
    pub fn set_mimetypes(&mut self, mimetypes: Vec<String>) {
        self.mimetypes = mimetypes;
    }

    /// Get the cached MIME type list for this selection source.
    /// Mirrors meta_selection_source_get_mime_types.
    pub fn get_mimetypes(&self) -> &[String] {
        &self.mimetypes
    }

    /// Check whether this source offers the given MIME type.
    pub fn has_mimetype(&self, mimetype: &str) -> bool {
        self.mimetypes.iter().any(|m| m.as_str() == mimetype)
    }

    /// Get the underlying Wayland data source pointer.
    pub fn get_data_source(&self) -> *mut core::ffi::c_void {
        self.data_source
    }

    /// Number of MIME types offered by this source.
    pub fn mimetype_count(&self) -> usize {
        self.mimetypes.len()
    }
}
