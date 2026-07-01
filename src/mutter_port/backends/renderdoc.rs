//! Renderdoc — RenderDoc integration for debugging from GNOME Mutter
//!
//! Provides optional RenderDoc (graphics debugger) capture integration.
//! Allows marking capture points for GPU timeline analysis.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-renderdoc.h

/// MetaRenderdoc — RenderDoc capture controller.
/// Queues frame captures via the RenderDoc C API (RENDERDOC_API_1_1_2).
#[derive(Debug, Clone)]
pub struct MetaRenderdoc {
    /// Backend reference (MetaBackend *)
    pub backend: *mut core::ffi::c_void,
    /// Hash table of queued views for capture (opaque GHashTable *)
    pub queued_views: *mut core::ffi::c_void,
    /// Whether RenderDoc is connected and API available
    pub connected: u32,
    /// RenderDoc API vtable (RENDERDOC_API_1_1_2 *)
    pub api: *mut core::ffi::c_void,
}

impl MetaRenderdoc {
    pub fn new() -> Self {
        MetaRenderdoc {
            backend: core::ptr::null_mut(),
            queued_views: core::ptr::null_mut(),
            connected: 0,
            api: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaRenderdoc {
    fn default() -> Self {
        Self::new()
    }
}
