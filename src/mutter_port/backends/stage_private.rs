//! Stage Private — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stage-private.h

use alloc::string::String;

/// MetaStageWatchPhase
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaStageWatchPhase {
    META_STAGE_WATCH_BEFORE_PAINT,
    META_STAGE_WATCH_AFTER_ACTOR_PAINT,
    META_STAGE_WATCH_AFTER_OVERLAY_PAINT,
    META_STAGE_WATCH_AFTER_PAINT,
    META_STAGE_WATCH_SKIPPED_PAINT,
}

// TODO: Extract struct definitions from C header
// TODO: Add type definitions and implementations