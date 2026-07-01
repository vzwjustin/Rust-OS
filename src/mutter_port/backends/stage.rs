//! Stage ported from GNOME Mutter's src/backends/meta-stage.c
//!
//! The Mutter stage is a specialization of the Clutter stage. It manages cursor
//! overlays (drawn above the scene) and a set of "watchers" invoked at defined
//! paint phases. It also drives redraws when a cursor overlay changes and on
//! power-save transitions.
//!
//! Stubbed: Cogl pipelines/textures/framebuffers, Clutter paint contexts and
//! actor redraw scheduling. The overlay state model, watch phases, and the
//! cursor-rect-to-clip geometry are ported faithfully.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stage.c

use alloc::vec::Vec;

use super::logical_monitor::MtkRectangle;
use super::stage_view::PaintFlag;

/// A 4x4 transform matrix (stub for graphene_matrix_t). Stored to detect change.
pub type Matrix = [[f32; 4]; 4];

/// A floating-point rectangle (stub for graphene_rect_t).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Paint phase at which a stage watch callback fires. Mirrors MetaStageWatchPhase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageWatchPhase {
    BeforePaint,
    AfterActorPaint,
    AfterOverlayPaint,
    AfterPaint,
    SkippedPaint,
}

/// Number of watch phases (mirrors META_N_WATCH_MODES).
pub const N_WATCH_MODES: usize = 5;

impl StageWatchPhase {
    fn index(self) -> usize {
        match self {
            StageWatchPhase::BeforePaint => 0,
            StageWatchPhase::AfterActorPaint => 1,
            StageWatchPhase::AfterOverlayPaint => 2,
            StageWatchPhase::AfterPaint => 3,
            StageWatchPhase::SkippedPaint => 4,
        }
    }
}

/// Per-view paint state for an overlay (mirrors MetaOverlayViewState).
#[derive(Debug, Clone, Copy, Default)]
struct OverlayViewState {
    painted_rect: RectF,
    has_painted_rect: bool,
}

/// A cursor overlay: a texture drawn above the stage at a destination rect.
///
/// The Cogl pipeline/texture are stubbed as opaque ids; `transform` is retained
/// to reproduce the "only re-set the pipeline matrix on change" logic.
#[derive(Debug)]
pub struct Overlay {
    pub is_visible: bool,
    /// Some(texture_id) when a texture is bound; None when cleared.
    pub texture: Option<u64>,
    transform: Matrix,
    current_rect: RectF,
    /// Per-view paint state keyed by view id.
    view_states: Vec<(u64, OverlayViewState)>,
}

impl Overlay {
    /// Mirrors meta_overlay_new.
    pub fn new() -> Self {
        Overlay {
            is_visible: false,
            texture: None,
            transform: [[0.0; 4]; 4],
            current_rect: RectF::default(),
            view_states: Vec::new(),
        }
    }

    /// Update the overlay's texture, transform, and destination rect.
    /// Faithful port of meta_overlay_set: the pipeline matrix is only updated
    /// when the transform actually changes.
    pub fn set(&mut self, texture: Option<u64>, matrix: &Matrix, dst_rect: &RectF) {
        if self.texture != texture {
            self.texture = texture;
            // cogl_pipeline_set_layer_texture(pipeline, 0, texture) - stubbed.
        }

        if !matrix_equal(matrix, &self.transform) {
            // cogl_pipeline_set_layer_matrix(pipeline, 0, matrix) - stubbed.
            self.transform = *matrix;
        }

        self.current_rect = *dst_rect;
    }

    /// Ensure and return the per-view paint state (mirrors ensure_view_state).
    /// Used by the (stubbed) cursor redraw path.
    #[allow(dead_code)]
    fn get_view_state(&mut self, view_id: u64) -> &mut OverlayViewState {
        if let Some(idx) = self.view_states.iter().position(|(id, _)| *id == view_id) {
            &mut self.view_states[idx].1
        } else {
            self.view_states
                .push((view_id, OverlayViewState::default()));
            let last = self.view_states.len() - 1;
            &mut self.view_states[last].1
        }
    }

    /// Forget all per-view paint state. Mirrors meta_overlay_invalidate_views.
    pub fn invalidate_views(&mut self) {
        self.view_states.clear();
    }
}

impl Default for Overlay {
    fn default() -> Self {
        Self::new()
    }
}

/// A registered watch: a callback bound to a phase and optionally a single view.
///
/// The function pointer + user_data of the C original are represented by an
/// opaque `id` the caller can dispatch on.
#[derive(Debug, Clone, Copy)]
pub struct StageWatch {
    pub id: u64,
    /// When Some, the watch only fires for that view id.
    pub view_id: Option<u64>,
    pub phase: StageWatchPhase,
}

/// The Mutter stage.
#[derive(Debug)]
pub struct Stage {
    /// Opaque backend handle (MetaBackend pointer in Mutter). Stubbed as id.
    pub backend_id: u64,
    /// Watchers per phase.
    watchers: [Vec<StageWatch>; N_WATCH_MODES],
    /// Cursor overlays, most-recently-created first (prepend order).
    overlays: Vec<Overlay>,
    next_watch_id: u64,
    /// Stage size in pixels, updated by rebuild_views.
    pub width: i32,
    pub height: i32,
}

impl Stage {
    /// Mirrors meta_stage_new + meta_stage_init.
    pub fn new(backend_id: u64) -> Self {
        Stage {
            backend_id,
            watchers: Default::default(),
            overlays: Vec::new(),
            next_watch_id: 1,
            width: 0,
            height: 0,
        }
    }

    /// Fire every watcher registered for `phase` that matches `view_id`.
    ///
    /// Faithful to notify_watchers_for_mode: a view-scoped watch is skipped when
    /// the current view differs. Returns the ids of the watches that fired so the
    /// caller can dispatch the real callbacks.
    pub fn notify_watchers_for_mode(
        &self,
        view_id: Option<u64>,
        phase: StageWatchPhase,
    ) -> Vec<u64> {
        let mut fired = Vec::new();
        for watch in &self.watchers[phase.index()] {
            if let (Some(w_view), Some(v)) = (watch.view_id, view_id) {
                if w_view != v {
                    continue;
                }
            }
            fired.push(watch.id);
        }
        fired
    }

    /// Create a new cursor overlay, prepended to the overlay list.
    /// Faithful port of meta_stage_create_cursor_overlay; returns the index.
    pub fn create_cursor_overlay(&mut self) -> usize {
        self.overlays.insert(0, Overlay::new());
        0
    }

    /// Remove a cursor overlay by index. Mirrors meta_stage_remove_cursor_overlay.
    pub fn remove_cursor_overlay(&mut self, index: usize) {
        if index < self.overlays.len() {
            self.overlays.remove(index);
        }
    }

    /// Update a cursor overlay and queue redraws for affected views.
    /// Faithful port of meta_stage_update_cursor_overlay.
    pub fn update_cursor_overlay(
        &mut self,
        index: usize,
        texture: Option<u64>,
        matrix: &Matrix,
        dst_rect: &RectF,
    ) {
        if let Some(overlay) = self.overlays.get_mut(index) {
            overlay.set(texture, matrix, dst_rect);
        }
        // queue_redraw_for_cursor_overlay(...) - view iteration/redraw stubbed.
    }

    pub fn overlay_set_visible(&mut self, index: usize, is_visible: bool) {
        if let Some(overlay) = self.overlays.get_mut(index) {
            if overlay.is_visible == is_visible {
                return;
            }
            overlay.is_visible = is_visible;
            // queue_redraw_for_cursor_overlay(...) - stubbed.
        }
    }

    /// Register a watch for a paint phase. Faithful to meta_stage_watch_view.
    pub fn watch_view(&mut self, view_id: Option<u64>, phase: StageWatchPhase) -> StageWatch {
        let watch = StageWatch {
            id: self.next_watch_id,
            view_id,
            phase,
        };
        self.next_watch_id += 1;
        self.watchers[phase.index()].push(watch);
        watch
    }

    /// Remove a watch by id from whichever phase holds it.
    /// Faithful to meta_stage_remove_watch (asserts the watch existed).
    pub fn remove_watch(&mut self, watch_id: u64) {
        let mut removed = false;
        for phase in &mut self.watchers {
            if let Some(pos) = phase.iter().position(|w| w.id == watch_id) {
                phase.swap_remove(pos);
                removed = true;
                break;
            }
        }
        debug_assert!(removed);
    }

    /// Rebuild views and resize the stage to the screen size.
    ///
    /// Faithful port of meta_stage_rebuild_views: sets the stage size and
    /// invalidates every overlay's per-view state. The stage-impl/monitor-manager
    /// hardware calls are stubbed; the caller supplies the screen size.
    pub fn rebuild_views(&mut self, width: i32, height: i32) {
        self.width = width;
        self.height = height;
        for overlay in &mut self.overlays {
            overlay.invalidate_views();
        }
    }

    /// React to a power-save change: queue a redraw when power save turns on.
    /// Mirrors on_power_save_changed (redraw only when entering POWER_SAVE_ON).
    pub fn on_power_save_changed(&self, power_save_on: bool) {
        if power_save_on {
            // clutter_actor_queue_redraw(stage) - stubbed.
        }
    }
}

/// Whether two matrices are (fast-)equal. Mirrors graphene_matrix_equal_fast.
fn matrix_equal(a: &Matrix, b: &Matrix) -> bool {
    a == b
}

/// Convert a cursor rect to an integer clip rect, growing to cover the floored
/// coordinates. Faithful port of cursor_rect_to_clip.
pub fn cursor_rect_to_clip(cursor_rect: &RectF) -> MtkRectangle {
    // MTK_ROUNDING_STRATEGY_GROW: floor origin, ceil the far edge.
    let x = libm_floorf(cursor_rect.x) as i32;
    let y = libm_floorf(cursor_rect.y) as i32;
    let right = libm_ceilf(cursor_rect.x + cursor_rect.width) as i32;
    let bottom = libm_ceilf(cursor_rect.y + cursor_rect.height) as i32;
    let mut clip = MtkRectangle::new(x, y, right - x, bottom - y);

    // Enlarge by twice the fractional difference floored away, as in the C.
    clip.width += (libm_ceilf(cursor_rect.x - clip.x as f32) as i32) * 2;
    clip.height += (libm_ceilf(cursor_rect.y - clip.y as f32) as i32) * 2;
    clip
}

/// floorf without libm dependency (finite, non-huge inputs).
fn libm_floorf(v: f32) -> f32 {
    let t = v as i64 as f32;
    if t > v {
        t - 1.0
    } else {
        t
    }
}

/// ceilf without libm dependency (finite, non-huge inputs).
fn libm_ceilf(v: f32) -> f32 {
    let t = v as i64 as f32;
    if t < v {
        t + 1.0
    } else {
        t
    }
}

/// Whether a view's default paint flags allow cursor painting.
/// Helper reflecting the CLUTTER_PAINT_FLAG_NO_CURSORS check in the C.
pub fn allows_cursor(paint_flags: PaintFlag) -> bool {
    paint_flags != PaintFlag::NoCursors
}
