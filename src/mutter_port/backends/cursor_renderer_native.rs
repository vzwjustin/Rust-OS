//! Cursor Renderer Native ported from GNOME Mutter's src/backends/
//!
//! Native backend cursor renderer using hardware cursors via DRM/KMS.
//! Prepares cursor frames for hardware scanout on native displays.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-cursor-renderer-native.c

/// Native DRM/KMS hardware cursor renderer.
pub struct MetaCursorRendererNative {
    // TODO: drm_cursors, current_cursor, pending_updates from upstream
}

impl MetaCursorRendererNative {
    /// Create a new native cursor renderer.
    pub fn new() -> Self {
        MetaCursorRendererNative {}
    }

    /// Prepare cursor frame for renderer view.
    pub fn prepare_frame(&mut self) {
        // TODO: Render cursor sprite to hardware buffer
    }
}

impl Default for MetaCursorRendererNative {
    fn default() -> Self {
        Self::new()
    }
}
