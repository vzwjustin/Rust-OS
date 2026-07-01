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
//!   objects would be supplied by a future Cogl-equivalent rendering backend.
//! - **`ClutterStageView`**: likewise no stage-view type exists yet. A
//!   minimal opaque `StageView` placeholder is defined locally, holding only
//!   what `PaintContext::new_for_view` needs conceptually (nothing — the
//!   real type exposes `get_color_state`/`get_framebuffer`, which callers
//!   would supply; ported as constructor parameters instead of being pulled
//!   from the view, to avoid inventing those APIs here).
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
/// stack structure (push/pop/get/get_base) ports correctly. The
/// `Framebuffer` is a unit struct (all framebuffers compare equal), which
/// is sufficient for the stack-depth logic in `is_drawing_off_stage`; a
/// future GPU backend would replace this with a real framebuffer handle
/// carrying a DRM/GEM buffer reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Framebuffer;

/// Port of `ClutterStageView`.
///
/// Describes a single output/monitor the stage is rendered onto. The real
/// C type exposes `get_color_state()` and `get_framebuffer()`; until those
/// subsystems are ported, callers of [`PaintContext::new_for_view`] pass
/// the color state and framebuffer in explicitly, and this struct tracks
/// the geometric/monitor metadata a stage view carries.
#[derive(Debug, Clone, PartialEq)]
pub struct StageView {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    scale: f32,
    primary: bool,
    logical_monitor_index: i32,
}

impl StageView {
    pub const fn new(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        scale: f32,
        primary: bool,
        logical_monitor_index: i32,
    ) -> StageView {
        StageView {
            x,
            y,
            width,
            height,
            scale,
            primary,
            logical_monitor_index,
        }
    }

    pub fn x(&self) -> i32 {
        self.x
    }

    pub fn y(&self) -> i32 {
        self.y
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn primary(&self) -> bool {
        self.primary
    }

    pub fn logical_monitor_index(&self) -> i32 {
        self.logical_monitor_index
    }
}

/// Electro-optical transfer function: how encoded values map to light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eotf {
    SRGB,
    PQ,
    Linear,
}

/// Transfer function used to encode color values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferFunction {
    SRGB,
    PQ,
    Linear,
}

/// Color gamut describing the primaries of a color space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gamut {
    SRGB,
    Bt2020,
}

/// Port of `ClutterColorState`.
///
/// Tracks the color-management description of a render target: the
/// transfer function used to encode values, the color gamut of the
/// primaries, and the electro-optical transfer function describing how
/// encoded values map to light.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorState {
    transfer_function: TransferFunction,
    gamut: Gamut,
    eotf: Eotf,
}

impl ColorState {
    pub const fn new(transfer_function: TransferFunction, gamut: Gamut, eotf: Eotf) -> ColorState {
        ColorState {
            transfer_function,
            gamut,
            eotf,
        }
    }

    pub const fn srgb() -> ColorState {
        ColorState::new(TransferFunction::SRGB, Gamut::SRGB, Eotf::SRGB)
    }

    pub fn transfer_function(&self) -> TransferFunction {
        self.transfer_function
    }

    pub fn gamut(&self) -> Gamut {
        self.gamut
    }

    pub fn eotf(&self) -> Eotf {
        self.eotf
    }
}

impl Default for ColorState {
    fn default() -> ColorState {
        ColorState::srgb()
    }
}

/// Port of `ClutterFrame` (a frame-clock frame).
///
/// Tracks per-frame timing state from the stage's frame clock: the
/// timestamp of the last presentation, the refresh rate of the output, a
/// monotonically increasing frame counter, and how many frames are
/// currently pending presentation.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    last_presentation_time: i64,
    refresh_rate: f32,
    frame_counter: u64,
    pending_frame_count: u32,
}

impl Frame {
    pub const fn new(refresh_rate: f32) -> Frame {
        Frame {
            last_presentation_time: 0,
            refresh_rate,
            frame_counter: 0,
            pending_frame_count: 0,
        }
    }

    pub fn last_presentation_time(&self) -> i64 {
        self.last_presentation_time
    }

    pub fn refresh_rate(&self) -> f32 {
        self.refresh_rate
    }

    pub fn frame_counter(&self) -> u64 {
        self.frame_counter
    }

    pub fn pending_frame_count(&self) -> u32 {
        self.pending_frame_count
    }

    /// Records that a frame was presented at `time`, advancing the frame
    /// counter and decrementing the pending count.
    pub fn record_presentation(&mut self, time: i64) {
        self.last_presentation_time = time;
        self.frame_counter = self.frame_counter.saturating_add(1);
        if self.pending_frame_count > 0 {
            self.pending_frame_count -= 1;
        }
    }

    /// Marks a frame as pending presentation.
    pub fn begin_frame(&mut self) {
        self.pending_frame_count = self.pending_frame_count.saturating_add(1);
    }

    /// Returns the interval between frames in microseconds for the current
    /// refresh rate (1e6 / refresh_rate).
    pub fn frame_interval_us(&self) -> i64 {
        if self.refresh_rate <= 0.0 {
            return 0;
        }
        (1_000_000.0 / self.refresh_rate) as i64
    }
}

/// A single clipping plane defined by a normal and a signed distance from
/// the origin (`normal . p + distance >= 0` is the inside half-space).
#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    normal: [f32; 3],
    distance: f32,
}

impl Plane {
    pub const fn new(normal: [f32; 3], distance: f32) -> Plane {
        Plane { normal, distance }
    }

    /// Returns true if the point lies on the inside half-space of the plane.
    pub fn inside(&self, p: &[f32; 3]) -> bool {
        self.normal[0] * p[0] + self.normal[1] * p[1] + self.normal[2] * p[2] + self.distance >= 0.0
    }
}

/// Port of a single entry of the C `clip_frusta` `GArray`.
///
/// Represents a view frustum as six clipping planes (left, right, bottom,
/// top, near, far). A point is inside the frustum when it lies on the
/// inside half-space of all six planes.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipFrustum {
    planes: [Plane; 6],
}

impl ClipFrustum {
    pub const fn from_planes(planes: [Plane; 6]) -> ClipFrustum {
        ClipFrustum { planes }
    }

    /// Builds an axis-aligned frustum from near-plane bounds (left, right,
    /// bottom, top) and near/far distances, with the eye at the origin
    /// looking down -z.
    pub fn from_bounds(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> ClipFrustum {
        ClipFrustum::from_planes([
            // left: x >= left  ->  -x + left <= 0  ->  x - left >= 0
            Plane::new([1.0, 0.0, 0.0], -left),
            // right: x <= right ->  x - right <= 0 ->  -x + right >= 0
            Plane::new([-1.0, 0.0, 0.0], right),
            // bottom: y >= bottom
            Plane::new([0.0, 1.0, 0.0], -bottom),
            // top: y <= top
            Plane::new([0.0, -1.0, 0.0], top),
            // near: z <= -near  ->  -z - near >= 0
            Plane::new([0.0, 0.0, -1.0], -near),
            // far: z >= -far  ->  z + far >= 0
            Plane::new([0.0, 0.0, 1.0], far),
        ])
    }

    pub fn planes(&self) -> &[Plane; 6] {
        &self.planes
    }

    /// Returns true if the point lies inside the frustum.
    pub fn contains_point(&self, p: &[f32; 3]) -> bool {
        self.planes.iter().all(|plane| plane.inside(p))
    }

    /// Returns true if the axis-aligned bounding box (min, max) intersects
    /// the frustum, using the standard p-vertex/n-vertex test against each
    /// plane.
    pub fn intersects_aabb(&self, min: &[f32; 3], max: &[f32; 3]) -> bool {
        for plane in &self.planes {
            // p-vertex: the corner of the AABB most along the normal
            // direction; n-vertex: the corner most against it. The box is
            // outside the plane if even the p-vertex is outside.
            let p = [
                if plane.normal[0] >= 0.0 {
                    max[0]
                } else {
                    min[0]
                },
                if plane.normal[1] >= 0.0 {
                    max[1]
                } else {
                    min[1]
                },
                if plane.normal[2] >= 0.0 {
                    max[2]
                } else {
                    min[2]
                },
            ];
            if !plane.inside(&p) {
                return false;
            }
        }
        true
    }
}

impl Default for ClipFrustum {
    fn default() -> ClipFrustum {
        ClipFrustum::from_bounds(-1.0, 1.0, -1.0, 1.0, 0.0, 1.0)
    }
}

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

    fn dummy_view() -> StageView {
        StageView::new(0, 0, 1920, 1080, 1.0, true, 0)
    }

    #[test]
    fn new_for_view_pushes_initial_framebuffer_and_color_state() {
        let ctx = PaintContext::new_for_view(
            dummy_view(),
            &dummy_region(),
            Vec::new(),
            PaintFlag::NONE,
            ColorState::srgb(),
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
            ColorState::srgb(),
        );

        assert!(ctx.stage_view().is_none());
        assert!(ctx.is_drawing_off_stage());
        assert!(ctx.redraw_clip().is_some());
    }

    #[test]
    fn new_for_framebuffer_allows_no_redraw_clip() {
        let ctx = PaintContext::new_for_framebuffer(
            Framebuffer,
            None,
            PaintFlag::NONE,
            ColorState::srgb(),
        );

        assert!(ctx.redraw_clip().is_none());
    }

    #[test]
    fn framebuffer_push_pop_stack_order() {
        let mut ctx = PaintContext::new_for_framebuffer(
            Framebuffer,
            None,
            PaintFlag::NONE,
            ColorState::srgb(),
        );

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
        let mut ctx = PaintContext::new_for_framebuffer(
            Framebuffer,
            None,
            PaintFlag::NONE,
            ColorState::srgb(),
        );

        assert!(ctx.color_state().is_none());
        ctx.push_color_state(ColorState::srgb());
        assert!(ctx.color_state().is_some());
        ctx.pop_color_state();
        assert!(ctx.color_state().is_none());
    }

    #[test]
    fn target_color_state_stack_push_pop() {
        let mut ctx = PaintContext::new_for_framebuffer(
            Framebuffer,
            None,
            PaintFlag::NONE,
            ColorState::srgb(),
        );

        // The constructor already pushed one target color state.
        assert!(ctx.target_color_state().is_some());
        ctx.push_target_color_state(ColorState::srgb());
        ctx.pop_target_color_state();
        assert!(ctx.target_color_state().is_some());
    }

    #[test]
    fn frame_assignment() {
        let mut ctx = PaintContext::new_for_framebuffer(
            Framebuffer,
            None,
            PaintFlag::NONE,
            ColorState::srgb(),
        );

        assert!(ctx.frame().is_none());
        ctx.assign_frame(Frame::new(60.0));
        assert!(ctx.frame().is_some());
    }

    #[test]
    fn stage_view_tracks_geometry_and_monitor() {
        let view = StageView::new(100, 200, 3840, 2160, 2.0, false, 1);
        assert_eq!(view.x(), 100);
        assert_eq!(view.y(), 200);
        assert_eq!(view.width(), 3840);
        assert_eq!(view.height(), 2160);
        assert_eq!(view.scale(), 2.0);
        assert!(!view.primary());
        assert_eq!(view.logical_monitor_index(), 1);
    }

    #[test]
    fn color_state_accessors() {
        let cs = ColorState::new(TransferFunction::PQ, Gamut::Bt2020, Eotf::PQ);
        assert_eq!(cs.transfer_function(), TransferFunction::PQ);
        assert_eq!(cs.gamut(), Gamut::Bt2020);
        assert_eq!(cs.eotf(), Eotf::PQ);
    }

    #[test]
    fn frame_clock_timing() {
        let mut frame = Frame::new(60.0);
        assert_eq!(frame.frame_counter(), 0);
        assert_eq!(frame.pending_frame_count(), 0);
        frame.begin_frame();
        assert_eq!(frame.pending_frame_count(), 1);
        frame.record_presentation(16_666);
        assert_eq!(frame.frame_counter(), 1);
        assert_eq!(frame.pending_frame_count(), 0);
        assert_eq!(frame.last_presentation_time(), 16_666);
        assert_eq!(frame.frame_interval_us(), 16_666);
    }

    #[test]
    fn clip_frustum_contains_and_rejects_points() {
        let f = ClipFrustum::from_bounds(-1.0, 1.0, -1.0, 1.0, 0.0, 10.0);
        // Inside the frustum (z in [-10, 0]).
        assert!(f.contains_point(&[0.0, 0.0, -5.0]));
        // Outside each boundary.
        assert!(!f.contains_point(&[2.0, 0.0, -5.0]));
        assert!(!f.contains_point(&[0.0, 2.0, -5.0]));
        assert!(!f.contains_point(&[0.0, 0.0, 1.0]));
        assert!(!f.contains_point(&[0.0, 0.0, -11.0]));
    }

    #[test]
    fn clip_frustum_aabb_intersection() {
        let f = ClipFrustum::from_bounds(-1.0, 1.0, -1.0, 1.0, 0.0, 10.0);
        // Box fully inside.
        assert!(f.intersects_aabb(&[-0.5, -0.5, -5.0], &[0.5, 0.5, -4.0]));
        // Box fully outside (beyond right plane).
        assert!(!f.intersects_aabb(&[2.0, -0.5, -5.0], &[3.0, 0.5, -4.0]));
        // Box fully beyond far plane.
        assert!(!f.intersects_aabb(&[-0.5, -0.5, -20.0], &[0.5, 0.5, -15.0]));
    }

    #[test]
    fn paint_flag_union_and_contains() {
        let flags = PaintFlag::NO_CURSORS.union(PaintFlag::CLEAR);
        assert!(flags.contains(PaintFlag::NO_CURSORS));
        assert!(flags.contains(PaintFlag::CLEAR));
        assert!(!flags.contains(PaintFlag::FORCE_CURSORS));
    }
}
