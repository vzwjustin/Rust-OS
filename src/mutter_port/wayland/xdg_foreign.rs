//! Wayland XDG Foreign — cross-application surface handles.
//!
//! Implements xdg_foreign protocol for exporting and importing surface handles
//! between clients, enabling inter-application surface delegation.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-xdg-foreign.h

use alloc::{collections::BTreeMap, string::String, vec::Vec};

/// An exported surface handle: the opaque handle string and the surface
/// pointer that was exported via zxdg_exported_v1.
#[derive(Debug)]
pub struct ExportedHandle {
    /// Opaque handle string returned to the exporting client.
    pub handle: String,
    /// MetaWaylandSurface pointer that was exported.
    pub surface: *mut core::ffi::c_void,
    /// wl_resource pointer for the zxdg_exported_v1 object.
    pub resource: *mut core::ffi::c_void,
}

/// An imported surface handle: the handle string being imported and the
/// surface that should receive the delegated surface once resolved.
#[derive(Debug)]
pub struct ImportedHandle {
    /// Opaque handle string the importing client requested.
    pub handle: String,
    /// MetaWaylandSurface pointer that will receive the exported surface.
    pub surface: *mut core::ffi::c_void,
    /// wl_resource pointer for the zxdg_imported_v1 object.
    pub resource: *mut core::ffi::c_void,
    /// Whether the handle has been resolved to an exported surface.
    pub resolved: bool,
}

/// XDG foreign registry tracking exported and imported surface handles.
///
/// In the C original, `MetaWaylandXdgForeign` maintains a GHashTable of
/// exported handles keyed by handle string, and resolves imports by
/// looking up the same table. Here we use BTreeMap for deterministic
/// no_std-compatible lookup.
#[derive(Debug)]
pub struct MetaWaylandXdgForeign {
    /// Exported handles keyed by handle string.
    pub exported: BTreeMap<String, ExportedHandle>,
    /// Imported handles keyed by handle string.
    pub imported: BTreeMap<String, ImportedHandle>,
    /// Whether the protocol globals have been registered.
    pub initialized: bool,
}

impl MetaWaylandXdgForeign {
    /// Create a new empty XDG foreign registry.
    pub fn new() -> Self {
        MetaWaylandXdgForeign {
            exported: BTreeMap::new(),
            imported: BTreeMap::new(),
            initialized: false,
        }
    }

    /// Register an exported surface handle.
    /// A full implementation would generate a unique opaque handle string
    /// and emit the `zxdg_exported_v1.handle` event to the client.
    pub fn export_surface(
        &mut self,
        handle: String,
        surface: *mut core::ffi::c_void,
        resource: *mut core::ffi::c_void,
    ) {
        let handle_key = handle.clone();
        self.exported.insert(
            handle_key.clone(),
            ExportedHandle {
                handle,
                surface,
                resource,
            },
        );
        // Resolve any pending imports waiting on this handle.
        if let Some(imp) = self.imported.get_mut(&handle_key) {
            imp.resolved = true;
        }
    }

    /// Register an imported surface handle request.
    /// A full implementation would look up the handle in the exported
    /// table and, if found, emit `zxdg_imported_v1.imported` to the client.
    pub fn import_surface(
        &mut self,
        handle: String,
        surface: *mut core::ffi::c_void,
        resource: *mut core::ffi::c_void,
    ) -> bool {
        let resolved = self.exported.contains_key(&handle);
        self.imported.insert(
            handle.clone(),
            ImportedHandle {
                handle,
                surface,
                resource,
                resolved,
            },
        );
        resolved
    }

    /// Remove an exported handle from the registry.
    pub fn remove_exported(&mut self, handle: &str) -> Option<ExportedHandle> {
        self.exported.remove(handle)
    }

    /// Remove an imported handle from the registry.
    pub fn remove_imported(&mut self, handle: &str) -> Option<ImportedHandle> {
        self.imported.remove(handle)
    }

    /// Look up an exported handle by handle string.
    pub fn lookup_exported(&self, handle: &str) -> Option<&ExportedHandle> {
        self.exported.get(handle)
    }

    /// Look up an imported handle by handle string.
    pub fn lookup_imported(&self, handle: &str) -> Option<&ImportedHandle> {
        self.imported.get(handle)
    }

    /// Number of exported handles.
    pub fn exported_count(&self) -> usize {
        self.exported.len()
    }

    /// Number of imported handles.
    pub fn imported_count(&self) -> usize {
        self.imported.len()
    }

    /// Clear all exported and imported handles.
    pub fn clear(&mut self) {
        self.exported.clear();
        self.imported.clear();
    }
}

impl Default for MetaWaylandXdgForeign {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize XDG foreign support for the compositor.
///
/// Registers xdg_foreign and xdg_exporter protocols. Returns false if setup fails.
/// A full implementation would call wl_global_create for zxdg_exporter_v2
/// and zxdg_importer_v2.
pub fn meta_wayland_xdg_foreign_init(compositor: *mut core::ffi::c_void) -> bool {
    if compositor.is_null() {
        return false;
    }
    true
}

/// Finalize XDG foreign support.
///
/// Removes protocol handlers and cleans up exported surface handles.
/// A full implementation would destroy the wl_global objects and free
/// all exported/imported handle entries.
pub fn meta_wayland_xdg_foreign_finalize(_compositor: *mut core::ffi::c_void) {
    // Protocol globals and handle tables are owned by the compositor
    // struct; their Drop implementations handle cleanup.
}
