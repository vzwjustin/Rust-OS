//! Port of GNOME mutter's `clutter/clutter-stage*.{c,h}` family.
//!
//! This submodule groups the four stage-related modules:
//! - `stage_window` — the `ClutterStageWindow` interface (backend hook).
//! - `stage_manager` — the `ClutterStageManager` singleton tracking all
//!   stages.
//! - `stage_view` — `ClutterStageView`, one per monitor output.
//! - `stage` — `ClutterStage` itself, the top-level actor surface.
//!
//! See each file's `//!` header for what is ported vs. skipped.

pub mod stage;
pub mod stage_manager;
pub mod stage_view;
pub mod stage_window;
