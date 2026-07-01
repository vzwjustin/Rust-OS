//! Stage Impl — ported from GNOME Mutter
//!
//! Stage implementation for frame rendering and view management.
//! Wraps ClutterStageWindow and handles per-stage rendering operations.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stage-impl-private.h

/// Stage implementation structure.
pub struct MetaStageImpl;

impl MetaStageImpl {
    pub fn new() -> Self {
        MetaStageImpl
    }
}

impl Default for MetaStageImpl {
    fn default() -> Self {
        Self::new()
    }
}