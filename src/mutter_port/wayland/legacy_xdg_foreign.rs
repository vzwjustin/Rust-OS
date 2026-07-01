//! Wayland Legacy XDG Foreign module
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-legacy-xdg-foreign.h
//!
//! Provides legacy XDG foreign (surface handle export/import) protocol support.
//! This is the zxdg_foreign_v1 family (as opposed to the v2 in xdg_foreign.rs).
//! Tracks exported and imported surface handles for cross-application
//! surface delegation.

use alloc::{collections::BTreeMap, string::String};

/// An exported surface handle for the legacy foreign protocol.
#[derive(Debug)]
pub struct LegacyExportedHandle {
    /// Opaque handle string returned to the exporting client.
    pub handle: String,
    /// MetaWaylandSurface pointer that was exported.
    pub surface: *mut core::ffi::c_void,
    /// wl_resource pointer for the zxdg_exported_v1 object.
    pub resource: *mut core::ffi::c_void,
}

/// An imported surface handle for the legacy foreign protocol.
#[derive(Debug)]
pub struct LegacyImportedHandle {
    /// Opaque handle string the importing client requested.
    pub handle: String,
    /// MetaWaylandSurface pointer that will receive the exported surface.
    pub surface: *mut core::ffi::c_void,
    /// wl_resource pointer for the zxdg_imported_v1 object.
    pub resource: *mut core::ffi::c_void,
    /// Whether the handle has been resolved to an exported surface.
    pub resolved: bool,
}

/// Legacy XDG foreign registry tracking exported and imported surface handles.
///
/// Mirrors `MetaWaylandXdgForeign` but for the v1 protocol family. Uses
/// BTreeMap for deterministic no_std-compatible lookup.
#[derive(Debug)]
pub struct MetaWaylandLegacyXdgForeign {
    /// Exported handles keyed by handle string.
    pub exported: BTreeMap<String, LegacyExportedHandle>,
    /// Imported handles keyed by handle string.
    pub imported: BTreeMap<String, LegacyImportedHandle>,
    /// Whether the protocol globals have been registered.
    pub initialized: bool,
}

impl MetaWaylandLegacyXdgForeign {
    /// Create a new empty legacy foreign registry.
    pub fn new() -> Self {
        MetaWaylandLegacyXdgForeign {
            exported: BTreeMap::new(),
            imported: BTreeMap::new(),
            initialized: false,
        }
    }

    /// Register an exported surface handle.
    /// A full implementation would generate a unique opaque handle string
    /// and emit the handle event to the client.
    pub fn export_surface(
        &mut self,
        handle: String,
        surface: *mut core::ffi::c_void,
        resource: *mut core::ffi::c_void,
    ) {
        let handle_key = handle.clone();
        self.exported.insert(
            handle_key.clone(),
            LegacyExportedHandle {
                handle,
                surface,
                resource,
            },
        );
        if let Some(imp) = self.imported.get_mut(&handle_key) {
            imp.resolved = true;
        }
    }

    /// Register an imported surface handle request.
    /// Returns true if the handle was immediately resolved.
    pub fn import_surface(
        &mut self,
        handle: String,
        surface: *mut core::ffi::c_void,
        resource: *mut core::ffi::c_void,
    ) -> bool {
        let resolved = self.exported.contains_key(&handle);
        self.imported.insert(
            handle.clone(),
            LegacyImportedHandle {
                handle,
                surface,
                resource,
                resolved,
            },
        );
        resolved
    }

    /// Remove an exported handle from the registry.
    pub fn remove_exported(&mut self, handle: &str) -> Option<LegacyExportedHandle> {
        self.exported.remove(handle)
    }

    /// Remove an imported handle from the registry.
    pub fn remove_imported(&mut self, handle: &str) -> Option<LegacyImportedHandle> {
        self.imported.remove(handle)
    }

    /// Look up an exported handle by handle string.
    pub fn lookup_exported(&self, handle: &str) -> Option<&LegacyExportedHandle> {
        self.exported.get(handle)
    }

    /// Look up an imported handle by handle string.
    pub fn lookup_imported(&self, handle: &str) -> Option<&LegacyImportedHandle> {
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

    /// Initialize legacy XDG foreign support for the compositor.
    /// A full implementation would call wl_global_create for the
    /// legacy zxdg_exporter_v1 and zxdg_importer_v1 globals.
    pub fn init(&mut self, compositor: *mut core::ffi::c_void) -> bool {
        if compositor.is_null() {
            return false;
        }
        self.initialized = true;
        true
    }
}

impl Default for MetaWaylandLegacyXdgForeign {
    fn default() -> Self {
        Self::new()
    }
}
