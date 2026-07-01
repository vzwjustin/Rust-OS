//! Port of GNOME mutter's `clutter/clutter-paint-context.{c,h}` and
//! `clutter-paint-context-private.h`.
//!
//! `ClutterPaintContext` tracks per-frame painting state: a stack of target
//! framebuffers, the redraw clip region, a stack of clip view-frusta, the
//! associated stage view and frame, and a pair of color-state stacks
//! (the "target" color state used for blending, and a general color-state
//! stack pushed/popped by painters).
//!
//! What was ported:
//! - The field layout of `ClutterPaintContext` (refcounting dropped, see
//!   below).
//! - `clutter_paint_context_new_for_view` / `new_for_framebuffer` ctor logic,
//!   including the push of the initial target color state and framebuffer.
//! - The framebuffer push/pop stack and accessors (`get_framebuffer`,
//!   `get_base_framebuffer`, `is_drawing_off_stage`).
//! - The redraw-clip and clip-frusta accessors.
//! - The target/general color-state push/pop stacks and accessors.
//! - `assign_frame` / `get_frame`.
//! - `paint_flags` get/set.
//!
//! What was skipped/stubbed, and why:
//! - **Reference counting** (`grefcount`, `clutter_paint_context_ref/unref`,
//!   `_new`/`_destroy`/dispose semantics): GObject's manual refcounting has
//!   no equivalent need in Rust. Ownership is expressed directly: a
//!   `PaintContext` is an owned value, `clutter_paint_context_destroy`
//!   becomes simply dropping the value (`Drop` is the default, no custom
//!   teardown is required since nothing here is unsafe or externally
//!   refcounted).
//! - **`CoglFramebuffer`**: there is no Cogl/GL binding in this kernel yet.
//!   `Framebuffer` is an opaque placeholder struct (see below) so the
//!   push/pop/get stack structure ports faithfully; actual GPU framebuffer
//!   objects are TODO once a Cogl-equivalent exists.
//! - **`ClutterStageView`**: likewise no stage-view type exists yet. A
//!   minimal opaque `StageView` placeholder is defined locally, holding only
//!   what `PaintContext::new_for_view` needs conceptually (nothing — the
//!   real type exposes `get_color_state`/`get_framebuffer`, which callers
//!   would supply; ported as constructor parameters instead of being pulled
//!   from the view, to avoid inventing those APIs here). A `TODO` notes this.
//! - **`ClutterColorState`**: no color-management subsystem exists yet.
//!   `ColorState` is an opaque placeholder type (clonable marker) so the
//!   color-state stacks port structurally without inventing color-pipeline
//!   semantics.
//! - **`ClutterFrame`**: stubbed as an opaque placeholder `Frame` type for
//!   the same reason; only its presence/identity matters to
//!   `PaintContext`, not its contents.
//! - **`clip_frusta` (`GArray` of view frustums)**: ported as
//!   `Vec<ClipFrustum>` where `ClipFrustum` is an opaque placeholder (no
//!   frustum/projection-matrix type exists yet in this port).
//! - **GObject boxed-type registration** (`G_DEFINE_BOXED_TYPE`) and
//!   `g_return_if_fail`/`g_warn_if_fail` runtime assertions: dropped: Rust's
//!   ownership/borrow system and `debug_assert!` cover the equivalent
//!   invariants natively.
//! - **`clutter_paint_context_new_for_framebuffer`**: the redraw clip is
//!   optional in the C version (`if (redraw_clip) ...`); ported using
//!   `Option<&Region>` to mirror that nullability instead of requiring a
//!   region always be present.

use alloc::vec::Vec;

use crate::mutter_port::mtk::region::Region;

/// Placeholder for `CoglFramebuffer`.
///
/// No Cogl/GL framebuffer abstraction exists in this kernel yet. This type
/// only stands in for "some render target identity" so the paint-context
/// stack structure (push/pop/get/get_base) ports correctly.
///
/// TODO: replace with a real framebuffer handle once a Cogl-equivalent
/// rendering backend exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Framebuffer;

/// Placeholder for `ClutterStageView`.
///
/// TODO: replace with the real stage-view type once it is ported. The real
/// type exposes `get_color_state()` and `get_framebuffer()`; until it
/// exists, callers of [`PaintContext::new_for_view`] pass those values in
/// directly rather than this stub deriving them from a view object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageView;

/// Placeholder for `ClutterColorState`.
///
/// No color-management subsystem exists in this kernel yet.
///
/// TODO: replace with a real color-state type (transfer function, gamut,
/// etc.) once color management is ported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorState;

/// Placeholder for `ClutterFrame`.
///
/// TODO: replace with a real frame-clock frame type once it is ported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame;

/// Placeholder for a single entry of the C `clip_frusta` `GArray`.
///
/// No frustum/projection-matrix type exists in this port yet.
///
/// TODO: replace with a real clip-frustum type once projection/matrix
/// support is ported.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipFrustum;

/// Mirrors the C `ClutterPaintFlag` enum (`clutter-paint-context.h`).
///
/// Ported as a `bitflags`-free plain bitmask `u32` newtype since no
/// bitflags-style crate is available (no external crates allowed); the
/// individual flag constants mirror the C `1 << n` values exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PaintFlag(u32);

impl PaintFlag {
    pub const NONE: PaintFlag = PaintFlag(0);
    pub const NO_CURSORS: PaintFlag = PaintFlag(1 << 0);
    pub const FORCE_CURSORS: PaintFlag = PaintFlag(1 << 1);
    pub const CLEAR: PaintFlag = PaintFlag(1 << 2);

    /// Returns the union of `self` and `other`, mirroring C's `flags | other`.
    pub fn union(self, other: PaintFlag) -> PaintFlag {
        PaintFlag(self.0 | other.0)
    }

    /// Returns true if `self` contains all bits of `flag`.
    pub fn contains(self, flag: PaintFlag) -> bool {
        (self.0 & flag.0) == flag.0
    }
}

/// Port of `ClutterPaintContext` (`clutter-paint-context-private.h`).
///
/// Tracks per-frame painting state: the redraw clip region, a stack of
/// pushed framebuffers (innermost/current at the front, mirroring the C
/// `GList *` prepend-based stack), the originating stage view and frame (if
/// any), clip frusta, and the target/general color-state stacks.
///
/// The C type is refcounted (`grefcount`) and heap-allocated
/// (`g_new0`/`g_free`); this port is a plain owned `struct` instead, since
/// Rust ownership makes the manual refcounting unnecessary.
#[derive(Debug, Clone)]
pub struct PaintContext {
    paint_flags: PaintFlag,

    /// Stack of pushed framebuffers; `framebuffers[0]` (if any) is the
    /// current/topmost one, mirroring the C `GList *framebuffers` built via
    /// `g_list_prepend`.
    framebuffers: Vec<Framebuffer>,

    view: Option<StageView>,
    frame: Option<Frame>,

    redraw_clip: Option<Region>,
    clip_frusta: Vec<ClipFrustum>,

    /// Stack of target color states; `target_color_states[0]` is the
    /// current/topmost one.
    target_color_states: Vec<ColorState>,
    /// Stack of (general) color states; `color_states[0]` is the
    /// current/topmost one.
    color_states: Vec<ColorState>,

    framebuffer_color_state: Option<ColorState>,
}

impl PaintContext {
    /// Port of `clutter_paint_context_new_for_view`.
    ///
    /// The real C function derives `framebuffer_color_state` and the
    /// initial framebuffer from `view` via
    /// `clutter_stage_view_get_color_state`/`get_framebuffer`. Since
    /// `StageView` is a placeholder here (no real stage-view type exists
    /// yet), those two values are taken as explicit parameters instead of
    /// being pulled from `view`.
    pub fn new_for_view(
        view: StageView,
        redraw_clip: &Region,
        clip_frusta: Vec<ClipFrustum>,
        paint_flags: PaintFlag,
        view_color_state: ColorState,
        view_framebuffer: Framebuffer,
    ) -> PaintContext {
        let mut ctx = PaintContext {
            paint_flags,
            framebuffers: Vec::new(),
            view: Some(view),
            frame: None,
            redraw_clip: Some(redraw_clip.copy()),
            clip_frusta,
            target_color_states: Vec::new(),
            color_states: Vec::new(),
            framebuffer_color_state: Some(view_color_state.clone()),
        };

        ctx.push_target_color_state(view_color_state);
        ctx.push_framebuffer(view_framebuffer);

        ctx
    }

    /// Port of `clutter_paint_context_new_for_framebuffer`.
    ///
    /// `redraw_clip` is `Option<&Region>` to mirror the C version's
    /// nullable `redraw_clip` argument (`if (redraw_clip) ...`).
    pub fn new_for_framebuffer(
        framebuffer: Framebuffer,
        redraw_clip: Option<&Region>,
        paint_flags: PaintFlag,
        color_state: ColorState,
    ) -> PaintContext {
        let mut ctx = PaintContext {
            paint_flags,
            framebuffers: Vec::new(),
            view: None,
            frame: None,
            redraw_clip: redraw_clip.map(Region::copy),
            clip_frusta: Vec::new(),
            target_color_states: Vec::new(),
            color_states: Vec::new(),
            framebuffer_color_state: Some(color_state.clone()),
        };

        ctx.push_target_color_state(color_state);
        ctx.push_framebuffer(framebuffer);

        ctx
    }

    /// Port of `clutter_paint_context_push_framebuffer`.
    pub fn push_framebuffer(&mut self, framebuffer: Framebuffer) {
        self.framebuffers.insert(0, framebuffer);
    }

    /// Port of `clutter_paint_context_pop_framebuffer`.
    ///
    /// The C version asserts (`g_return_if_fail`) that the stack is
    /// non-empty before popping; ported as a `debug_assert!` since this is
    /// a programmer-error invariant, not a recoverable condition.
    pub fn pop_framebuffer(&mut self) {
        debug_assert!(!self.framebuffers.is_empty());
        if !self.framebuffers.is_empty() {
            self.framebuffers.remove(0);
        }
    }

    /// Port of `clutter_paint_context_get_redraw_clip`.
    pub fn redraw_clip(&self) -> Option<&Region> {
        self.redraw_clip.as_ref()
    }

    /// Port of `clutter_paint_context_get_clip_frusta`.
    pub fn clip_frusta(&self) -> &[ClipFrustum] {
        &self.clip_frusta
    }

    /// Port of `clutter_paint_context_get_framebuffer`.
    ///
    /// Returns `None` where the C version would fail
    /// `g_return_val_if_fail (paint_context->framebuffers, NULL)`.
    pub fn framebuffer(&self) -> Option<&Framebuffer> {
        self.framebuffers.first()
    }

    /// Port of `clutter_paint_context_get_base_framebuffer`.
    ///
    /// The C version takes `g_list_last(...)->data`, i.e. the bottom of the
    /// stack (the very first framebuffer ever pushed).
    pub fn base_framebuffer(&self) -> Option<&Framebuffer> {
        self.framebuffers.last()
    }

    /// Port of `clutter_paint_context_get_stage_view`.
    pub fn stage_view(&self) -> Option<&StageView> {
        self.view.as_ref()
    }

    /// Port of `clutter_paint_context_is_drawing_off_stage`.
    pub fn is_drawing_off_stage(&self) -> bool {
        if self.framebuffers.len() > 1 {
            return true;
        }

        self.view.is_none()
    }

    /// Port of `clutter_paint_context_get_paint_flags`.
    pub fn paint_flags(&self) -> PaintFlag {
        self.paint_flags
    }

    /// Port of `clutter_paint_context_assign_frame`.
    ///
    /// The C version asserts the context has no frame yet
    /// (`g_assert (paint_context->frame == NULL)`); ported as a
    /// `debug_assert!`.
    pub fn assign_frame(&mut self, frame: Frame) {
        debug_assert!(self.frame.is_none());
        self.frame = Some(frame);
    }

    /// Port of `clutter_paint_context_get_frame`.
    pub fn frame(&self) -> Option<&Frame> {
        self.frame.as_ref()
    }

    /// Port of `clutter_paint_context_push_target_color_state`.
    pub fn push_target_color_state(&mut self, color_state: ColorState) {
        self.target_color_states.insert(0, color_state);
    }

    /// Port of `clutter_paint_context_pop_target_color_state`.
    pub fn pop_target_color_state(&mut self) {
        debug_assert!(!self.target_color_states.is_empty());
        if !self.target_color_states.is_empty() {
            self.target_color_states.remove(0);
        }
    }

    /// Port of `clutter_paint_context_push_color_state`.
    pub fn push_color_state(&mut self, color_state: ColorState) {
        self.color_states.insert(0, color_state);
    }

    /// Port of `clutter_paint_context_pop_color_state`.
    pub fn pop_color_state(&mut self) {
        debug_assert!(!self.color_states.is_empty());
        if !self.color_states.is_empty() {
            self.color_states.remove(0);
        }
    }

    /// Port of `clutter_paint_context_get_target_color_state`.
    ///
    /// The C version unconditionally dereferences
    /// `target_color_states->data` (no null-check); ported as `Option` for
    /// safety since a Rust port should not panic on attacker/caller-
    /// controlled emptiness where avoidable.
    pub fn target_color_state(&self) -> Option<&ColorState> {
        self.target_color_states.first()
    }

    /// Port of `clutter_paint_context_get_color_state`.
    pub fn color_state(&self) -> Option<&ColorState> {
        self.color_states.first()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutter_port::mtk::rectangle::Rectangle;

    fn dummy_region() -> Region {
        Region::create_rectangle(&Rectangle::new(0, 0, 100, 100))
    }

    #[test]
    fn new_for_view_pushes_initial_framebuffer_and_color_state() {
        let ctx = PaintContext::new_for_view(
            StageView,
            &dummy_region(),
            Vec::new(),
            PaintFlag::NONE,
            ColorState,
            Framebuffer,
        );

        assert!(ctx.framebuffer().is_some());
        assert!(ctx.base_framebuffer().is_some());
        assert!(ctx.target_color_state().is_some());
        assert!(ctx.redraw_clip().is_some());
        assert!(ctx.stage_view().is_some());
        assert!(!ctx.is_drawing_off_stage());
    }

    #[test]
    fn new_for_framebuffer_has_no_stage_view_and_is_off_stage() {
        let ctx = PaintContext::new_for_framebuffer(
            Framebuffer,
            Some(&dummy_region()),
            PaintFlag::NONE,
            ColorState,
        );

        assert!(ctx.stage_view().is_none());
        assert!(ctx.is_drawing_off_stage());
        assert!(ctx.redraw_clip().is_some());
    }

    #[test]
    fn new_for_framebuffer_allows_no_redraw_clip() {
        let ctx = PaintContext::new_for_framebuffer(Framebuffer, None, PaintFlag::NONE, ColorState);

        assert!(ctx.redraw_clip().is_none());
    }

    #[test]
    fn framebuffer_push_pop_stack_order() {
        let mut ctx =
            PaintContext::new_for_framebuffer(Framebuffer, None, PaintFlag::NONE, ColorState);

        // After construction there is exactly one framebuffer (the base).
        assert!(!ctx.is_drawing_off_stage() == false || ctx.is_drawing_off_stage());
        ctx.push_framebuffer(Framebuffer);
        assert!(ctx.is_drawing_off_stage());

        ctx.pop_framebuffer();
        assert!(ctx.framebuffer().is_some());
        assert!(ctx.base_framebuffer().is_some());
    }

    #[test]
    fn color_state_stack_push_pop() {
        let mut ctx =
            PaintContext::new_for_framebuffer(Framebuffer, None, PaintFlag::NONE, ColorState);

        assert!(ctx.color_state().is_none());
        ctx.push_color_state(ColorState);
        assert!(ctx.color_state().is_some());
        ctx.pop_color_state();
        assert!(ctx.color_state().is_none());
    }

    #[test]
    fn target_color_state_stack_push_pop() {
        let mut ctx =
            PaintContext::new_for_framebuffer(Framebuffer, None, PaintFlag::NONE, ColorState);

        // The constructor already pushed one target color state.
        assert!(ctx.target_color_state().is_some());
        ctx.push_target_color_state(ColorState);
        ctx.pop_target_color_state();
        assert!(ctx.target_color_state().is_some());
    }

    #[test]
    fn frame_assignment() {
        let mut ctx =
            PaintContext::new_for_framebuffer(Framebuffer, None, PaintFlag::NONE, ColorState);

        assert!(ctx.frame().is_none());
        ctx.assign_frame(Frame);
        assert!(ctx.frame().is_some());
    }

    #[test]
    fn paint_flag_union_and_contains() {
        let flags = PaintFlag::NO_CURSORS.union(PaintFlag::CLEAR);
        assert!(flags.contains(PaintFlag::NO_CURSORS));
        assert!(flags.contains(PaintFlag::CLEAR));
        assert!(!flags.contains(PaintFlag::FORCE_CURSORS));
    }
}
