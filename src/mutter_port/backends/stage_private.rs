//! Stage Private — ported from GNOME Mutter
//!
//! Stage management for rendering pipeline phases and view watching.
//! Provides hooks to observe different points in the paint/render cycle.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stage-private.h

use alloc::string::String;

/// Watch phase for observing stage rendering callbacks.
/// Allows observers to hook into different points of the paint pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaStageWatchPhase {
    /// Before any paint operations begin.
    BEFORE_PAINT = 0,
    /// After actor painting but before overlays.
    AFTER_ACTOR_PAINT = 1,
    /// After overlay painting.
    AFTER_OVERLAY_PAINT = 2,
    /// After all paint operations complete.
    AFTER_PAINT = 3,
    /// When paint was skipped (e.g., no damage).
    SKIPPED_PAINT = 4,
}

/// Number of watch phases.
pub const META_N_WATCH_MODES: u32 = 5;

/// Placeholder types for opaque C structures.
#[derive(Debug)]
pub struct MetaStage;

#[derive(Debug)]
pub struct MetaStageWatch;

#[derive(Debug)]
pub struct MetaOverlay;

/// Stage overlay for UI elements (opaque, used for cursor overlay, etc.).
#[derive(Debug)]
pub struct StageOverlay {
    /// Internal overlay handle (opaque).
    handle: u32,
}

impl StageOverlay {
    /// Create a new stage overlay.
    pub fn new() -> Self {
        StageOverlay { handle: 0 }
    }
}

impl Default for StageOverlay {
    fn default() -> Self {
        Self::new()
    }
}