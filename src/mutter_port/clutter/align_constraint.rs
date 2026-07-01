//! Port of GNOME mutter's `clutter/clutter-align-constraint.{c,h}`.
//!
//! `ClutterAlignConstraint` positions an actor relative to the size of a
//! "source" actor, using a normalized `factor` (`0.0` = left/top, `1.0` =
//! right/bottom) and an optional `pivot_point` (the point on the
//! constrained actor that gets aligned; `(-1, -1)` means "use `factor` as
//! the pivot", i.e. keep the actor inside the source).
//!
//! # What's ported
//!
//! - The `ClutterAlignConstraint` struct fields (`actor`, `source`,
//!   `align_axis`, `pivot`, `factor`) plus the `clutter_align_constraint_init`
//!   defaults (`align_axis = X_AXIS`, `pivot = (-1, -1)`, `factor = 0.0`).
//! - `clutter_align_constraint_new` / `set_source` / `set_align_axis` /
//!   `set_pivot_point` / `set_factor` / getters.
//! - `update_allocation`: the full `switch (align_axis)` over
//!   `X_AXIS`/`Y_AXIS`/`BOTH`, computing `offset_*_start` from the pivot
//!   (or `factor` when pivot is `-1`), then shifting the allocation and
//!   clamping to pixels via `ActorBox::clamp_to_pixel`.
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_FINAL_TYPE`, `GParamSpec` property
//!   install/notify, `set_property`/`get_property`, `dispose`): plain
//!   fields + setter methods.
//! - `source_queue_relayout` / `source_destroyed` signal handlers: these
//!   wire `queue-relayout`/`destroy` signals on the source so the bound
//!   actor relayouts when the source changes. No signal system in this
//!   port; the caller re-runs the constraint when the source changes. The
//!   `actor` back-pointer is kept for a future signal layer.
//! - `clutter_actor_contains (actor, source)` guard in `set_source` /
//!   `set_actor`: prevents binding to a descendant (which would create a
//!   cyclic layout). `ActorTree` has no `contains` (ancestor test) yet;
//!   the guard is omitted, so callers must avoid that configuration until
//!   `contains` lands. Documented here for visibility.
//! - `clutter_actor_queue_relayout` calls in the setters: not on
//!   `ActorTree` yet; setters just update fields.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use super::actor::{ActorId, ActorTree};
use super::actor_box::ActorBox;
use super::actor_meta::ActorMeta;
use super::constraint::Constraint;
use super::enums::AlignAxis;

/// Port of `graphene_point_t` as used by `ClutterAlignConstraint` — just
/// the two floats the constraint needs. `(-1.0, -1.0)` means "pivot
/// unset, use `factor` as the pivot" (matching `clutter_align_constraint_init`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PivotPoint {
    pub x: f32,
    pub y: f32,
}

impl PivotPoint {
    /// The "pivot unset" sentinel, matching `clutter_align_constraint_init`.
    pub const UNSET: Self = PivotPoint { x: -1.0, y: -1.0 };
}

/// Port of `ClutterAlignConstraint`.
#[derive(Debug, Clone)]
pub struct AlignConstraint {
    /// The bound actor back-pointer (C stores this for the relayout signal
    /// handler).
    pub actor: Option<ActorId>,
    /// The source actor whose size the alignment is relative to.
    pub source: Option<ActorId>,
    /// Which axis the alignment applies to.
    pub align_axis: AlignAxis,
    /// The pivot point on the constrained actor; `UNSET` means use `factor`.
    pub pivot: PivotPoint,
    /// The alignment factor in `[0.0, 1.0]`.
    pub factor: f32,
}

impl Default for AlignConstraint {
    fn default() -> Self {
        // Matches `clutter_align_constraint_init`.
        AlignConstraint {
            actor: None,
            source: None,
            align_axis: AlignAxis::XAxis,
            pivot: PivotPoint::UNSET,
            factor: 0.0,
        }
    }
}

impl AlignConstraint {
    /// `clutter_align_constraint_new`.
    pub fn new(source: Option<ActorId>, axis: AlignAxis, factor: f32) -> Self {
        AlignConstraint {
            actor: None,
            source,
            align_axis: axis,
            pivot: PivotPoint::UNSET,
            factor: factor.clamp(0.0, 1.0),
        }
    }

    /// `clutter_align_constraint_set_source` (minus the signal
    /// connect/disconnect and the `clutter_actor_contains` guard — see
    /// module docs).
    pub fn set_source(&mut self, source: Option<ActorId>) {
        self.source = source;
    }

    /// `clutter_align_constraint_get_source`.
    pub fn source(&self) -> Option<ActorId> {
        self.source
    }

    /// `clutter_align_constraint_set_align_axis` (minus the relayout queue).
    pub fn set_align_axis(&mut self, axis: AlignAxis) {
        self.align_axis = axis;
    }

    /// `clutter_align_constraint_get_align_axis`.
    pub fn align_axis(&self) -> AlignAxis {
        self.align_axis
    }

    /// `clutter_align_constraint_set_pivot_point` (minus the relayout
    /// queue). Validates that each component is either `-1.0` (unset) or
    /// in `[0.0, 1.0]`, matching the C `g_return_if_fail` guards.
    pub fn set_pivot_point(&mut self, pivot: PivotPoint) {
        debug_assert!(
            (pivot.x == -1.0 || (pivot.x >= 0.0 && pivot.x <= 1.0))
                && (pivot.y == -1.0 || (pivot.y >= 0.0 && pivot.y <= 1.0)),
            "pivot point out of range"
        );
        self.pivot = pivot;
    }

    /// `clutter_align_constraint_get_pivot_point`.
    pub fn pivot_point(&self) -> PivotPoint {
        self.pivot
    }

    /// `clutter_align_constraint_set_factor` (clamps to `[0,1]`, matching
    /// the C `CLAMP`; minus the relayout queue).
    pub fn set_factor(&mut self, factor: f32) {
        self.factor = factor.clamp(0.0, 1.0);
    }

    /// `clutter_align_constraint_get_factor`.
    pub fn factor(&self) -> f32 {
        self.factor
    }
}

impl Constraint for AlignConstraint {
    fn update_allocation(
        &mut self,
        _meta: &mut ActorMeta,
        _actor: ActorId,
        _allocation: &mut ActorBox,
    ) {
        // Needs the source's size from an `ActorTree`; the trait surface
        // can't borrow one. Use `update_allocation_with_tree`. See module
        // docs (same convention as `BindConstraint`).
    }
}

impl AlignConstraint {
    /// Tree-aware `update_allocation`, mirroring
    /// `clutter_align_constraint_update_allocation`. Reads the source's
    /// size from `tree` and shifts `allocation` per `align_axis`/`factor`/
    /// `pivot`, then clamps to pixels.
    pub fn update_allocation_with_tree(&self, tree: &ActorTree, allocation: &mut ActorBox) {
        let source = match self.source {
            Some(s) => s,
            None => return,
        };
        let (actor_w, actor_h) = (allocation.width(), allocation.height());
        let (source_w, source_h) = tree.get_size(source);

        let pivot_x = if self.pivot.x == -1.0 {
            self.factor
        } else {
            self.pivot.x
        };
        let pivot_y = if self.pivot.y == -1.0 {
            self.factor
        } else {
            self.pivot.y
        };

        let offset_x_start = pivot_x * -actor_w;
        let offset_y_start = pivot_y * -actor_h;

        match self.align_axis {
            AlignAxis::XAxis => {
                allocation.x1 += offset_x_start + source_w * self.factor;
                allocation.x2 = allocation.x1 + actor_w;
            }
            AlignAxis::YAxis => {
                allocation.y1 += offset_y_start + source_h * self.factor;
                allocation.y2 = allocation.y1 + actor_h;
            }
            AlignAxis::Both => {
                allocation.x1 += offset_x_start + source_w * self.factor;
                allocation.y1 += offset_y_start + source_h * self.factor;
                allocation.x2 = allocation.x1 + actor_w;
                allocation.y2 = allocation.y1 + actor_h;
            }
        }

        allocation.clamp_to_pixel();
    }
}

#[cfg(test)]
mod tests {
    use super::super::actor::{ActorCommon, NullBehavior};
    use super::*;
    use alloc::boxed::Box;

    fn leaf(w: f32, h: f32) -> Box<dyn super::super::actor::ActorBehavior> {
        Box::new(NullBehavior {
            natural_width: w,
            natural_height: h,
        })
    }

    fn make_src(tree: &mut ActorTree, w: f32, h: f32) -> ActorId {
        let id = tree.create(ActorCommon::default(), leaf(w, h));
        // Allocate so get_size reports the allocation.
        tree.allocate(id, ActorBox::new(0.0, 0.0, w, h));
        id
    }

    #[test]
    fn defaults_match_c_init() {
        let c = AlignConstraint::default();
        assert_eq!(c.align_axis, AlignAxis::XAxis);
        assert_eq!(c.pivot, PivotPoint::UNSET);
        assert_eq!(c.factor, 0.0);
        assert_eq!(c.source, None);
        assert_eq!(c.actor, None);
    }

    #[test]
    fn factor_clamped_to_unit_range() {
        let mut c = AlignConstraint::default();
        c.set_factor(2.0);
        assert_eq!(c.factor(), 1.0);
        c.set_factor(-1.0);
        assert_eq!(c.factor(), 0.0);
        c.set_factor(0.5);
        assert_eq!(c.factor(), 0.5);
    }

    #[test]
    fn x_axis_factor_zero_aligns_left() {
        let mut tree = ActorTree::new();
        let src = make_src(&mut tree, 100.0, 100.0);
        let c = AlignConstraint::new(Some(src), AlignAxis::XAxis, 0.0);
        let mut alloc = ActorBox::new(0.0, 0.0, 20.0, 20.0);
        c.update_allocation_with_tree(&tree, &mut alloc);
        // factor=0, pivot unset -> pivot_x=0 -> offset_x_start=0.
        // x1 += 0 + 100*0 = 0. y unchanged.
        assert_eq!(alloc, ActorBox::new(0.0, 0.0, 20.0, 20.0));
    }

    #[test]
    fn x_axis_factor_one_aligns_right() {
        let mut tree = ActorTree::new();
        let src = make_src(&mut tree, 100.0, 100.0);
        let c = AlignConstraint::new(Some(src), AlignAxis::XAxis, 1.0);
        let mut alloc = ActorBox::new(0.0, 0.0, 20.0, 20.0);
        c.update_allocation_with_tree(&tree, &mut alloc);
        // pivot_x=1 -> offset_x_start = -20. x1 += -20 + 100*1 = 80. x2=100.
        assert_eq!(alloc, ActorBox::new(80.0, 0.0, 100.0, 20.0));
    }

    #[test]
    fn x_axis_factor_half_centers() {
        let mut tree = ActorTree::new();
        let src = make_src(&mut tree, 100.0, 100.0);
        let c = AlignConstraint::new(Some(src), AlignAxis::XAxis, 0.5);
        let mut alloc = ActorBox::new(0.0, 0.0, 20.0, 20.0);
        c.update_allocation_with_tree(&tree, &mut alloc);
        // pivot_x=0.5 -> offset_x_start = -10. x1 += -10 + 50 = 40. x2=60.
        assert_eq!(alloc, ActorBox::new(40.0, 0.0, 60.0, 20.0));
    }

    #[test]
    fn both_axis_aligns_both() {
        let mut tree = ActorTree::new();
        let src = make_src(&mut tree, 100.0, 100.0);
        let c = AlignConstraint::new(Some(src), AlignAxis::Both, 1.0);
        let mut alloc = ActorBox::new(0.0, 0.0, 20.0, 20.0);
        c.update_allocation_with_tree(&tree, &mut alloc);
        // Both axes: x1=80, y1=80, x2=100, y2=100.
        assert_eq!(alloc, ActorBox::new(80.0, 80.0, 100.0, 100.0));
    }

    #[test]
    fn custom_pivot_overrides_factor_for_offset() {
        let mut tree = ActorTree::new();
        let src = make_src(&mut tree, 100.0, 100.0);
        let mut c = AlignConstraint::new(Some(src), AlignAxis::XAxis, 1.0);
        // pivot_x = 0 -> offset_x_start = 0 (instead of -20).
        c.set_pivot_point(PivotPoint { x: 0.0, y: -1.0 });
        let mut alloc = ActorBox::new(0.0, 0.0, 20.0, 20.0);
        c.update_allocation_with_tree(&tree, &mut alloc);
        // x1 += 0 + 100*1 = 100. x2 = 120.
        assert_eq!(alloc, ActorBox::new(100.0, 0.0, 120.0, 20.0));
    }
}
