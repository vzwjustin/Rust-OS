//! Color Store ported from GNOME Mutter's src/backends/
//!
//! Cache and repository for color profiles. Manages device profiles and
//! colord-backed profiles with async loading and caching.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-color-store.h

/// MetaColorStore — a G_DECLARE_FINAL_TYPE caching color profiles.
/// Opaque stub; real implementation in C backend.
pub struct MetaColorStore;

impl MetaColorStore {
    /// Create a new MetaColorStore (stub).
    pub fn new() -> Self {
        MetaColorStore
    }
}

impl Default for MetaColorStore {
    fn default() -> Self {
        Self::new()
    }
}
