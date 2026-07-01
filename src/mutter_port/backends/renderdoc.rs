//! Renderdoc — RenderDoc integration for debugging from GNOME Mutter
//!
//! Provides optional RenderDoc (graphics debugger) capture integration.
//! Allows marking capture points for GPU timeline analysis.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-renderdoc.h

/// MetaRenderdoc — RenderDoc capture controller.
/// Queues frame captures via the RenderDoc C API.
pub struct MetaRenderdoc {
    // TODO: port fields from meta-renderdoc.c (RenderDoc API handle, etc.)
}

impl MetaRenderdoc {
    pub fn new() -> Self {
        MetaRenderdoc {}
    }
}

impl Default for MetaRenderdoc {
    fn default() -> Self {
        Self::new()
    }
}