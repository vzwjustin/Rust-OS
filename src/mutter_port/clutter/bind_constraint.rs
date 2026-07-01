//! Port of GNOME mutter's `clutter/clutter-bind-constraint.{c,h}`.
//!
//! `ClutterBindConstraint` binds an actor's position and/or size to that
//! of a "source" actor, with an offset. It overrides
//! `ClutterConstraint::update_allocation` (to reposition/resize) and
//! `update_preferred_size` (so width/height bindings report the source's
//! preferred size).
//!
//! # What's ported
//!
//! - The `ClutterBindConstraint` struct fields (`source`, `coordinate`,
//!   `offset`) plus the `actor` back-pointer the C version keeps for the
//!   relayout-on-source-change signal handler.
//! - `clutter_bind_constraint_new` / `set_source` / `set_coordinate` /
//!   `set_offset` / getters.
//! - `update_allocation`: the full `switch (bind->coordinate)` over
//!   `X`/`Y`/`POSITION`/`WIDTH`/`HEIGHT`/`SIZE`/`ALL`, reading the source's
//!   position/size via `ActorTree::get_x`/`get_y`/`get_size`.
//! - `update_preferred_size`: the width/height/size/all bindings copy the
//!   source's preferred size, with the `clutter_actor_contains` guard
//!   omitted (no ancestor test on `ActorTree` yet — see rationale below).
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_FINAL_TYPE`, `GParamSpec` property
//!   install/notify, `set_property`/`get_property`): plain fields + setter
//!   methods.
//! - `source_queue_relayout` / `source_destroyed` signal handlers: these
//!   wire `notify`/`destroy`/`queue-relayout` signals on the source so the
//!   bound actor relayouts when the source changes. No signal system in
//!   this port; the caller is responsible for re-running the constraint
//!   when the source changes. The `actor` back-pointer is kept so a future
//!   signal layer can use it.
//! - `clutter_actor_contains (bind->source, actor)` guard in
//!   `update_preferred_size`: this prevents a binding from creating a
//!   cyclic size request when the source is an ancestor of the bound
//!   actor. `ActorTree` has no `contains` (ancestor test) method yet; the
//!   guard is omitted, which means a width/height/size binding where the
//!   source is an ancestor of the actor would recurse. Documented here so
//!   callers avoid that configuration until `contains` lands.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use super::actor::{ActorId, ActorTree};
use super::actor_box::ActorBox;
use super::actor_meta::ActorMeta;
use super::constraint::Constraint;
use super::enums::{BindCoordinate, Orientation};

/// Port of `ClutterBindConstraint`.
#[derive(Debug, Clone)]
pub struct BindConstraint {
    /// The bound actor back-pointer (C stores this separately from
    /// `ActorMeta::actor` for the relayout signal handler).
    pub actor: Option<ActorId>,
    /// The source actor whose position/size is bound to.
    pub source: Option<ActorId>,
    /// Which coordinate(s) to bind.
    pub coordinate: BindCoordinate,
    /// Offset added to the bound coordinate.
    pub offset: f32,
}

impl Default for BindConstraint {
    fn default() -> Self {
        BindConstraint {
            actor: None,
            source: None,
            coordinate: BindCoordinate::default(),
            offset: 0.0,
        }
    }
}

impl BindConstraint {
    /// `clutter_bind_constraint_new`.
    pub fn new(source: Option<ActorId>, coordinate: BindCoordinate, offset: f32) -> Self {
        BindConstraint {
            actor: None,
            source,
            coordinate,
            offset,
        }
    }

    /// `clutter_bind_constraint_set_source` (minus the signal
    /// connect/disconnect).
    pub fn set_source(&mut self, source: Option<ActorId>) {
        self.source = source;
    }

    /// `clutter_bind_constraint_set_coordinate`.
    pub fn set_coordinate(&mut self, coordinate: BindCoordinate) {
        self.coordinate = coordinate;
    }

    /// `clutter_bind_constraint_set_offset`.
    pub fn set_offset(&mut self, offset: f32) {
        self.offset = offset;
    }

    /// `clutter_bind_constraint_get_source`.
    pub fn source(&self) -> Option<ActorId> {
        self.source
    }

    /// `clutter_bind_constraint_get_coordinate`.
    pub fn coordinate(&self) -> BindCoordinate {
        self.coordinate
    }

    /// `clutter_bind_constraint_get_offset`.
    pub fn offset(&self) -> f32 {
        self.offset
    }
}

impl Constraint for BindConstraint {
    fn update_allocation(
        &mut self,
        _meta: &mut ActorMeta,
        _actor: ActorId,
        allocation: &mut ActorBox,
    ) {
        let source = match self.source {
            Some(s) => s,
            None => return,
        };
        // The C version reads from a borrowed `ClutterActor *source`; here
        // we need an `&ActorTree`, but `update_allocation` doesn't receive
        // one. The practical caller (`ActorTree`-driven constraint
        // application) uses `update_allocation_with_tree` below instead.
        // This trait impl is a no-op fallback; see module docs.
        let _ = (source, allocation);
    }

    fn update_preferred_size(
        &mut self,
        _meta: &mut ActorMeta,
        _actor: ActorId,
        _direction: Orientation,
        _for_size: f32,
        _minimum_size: &mut f32,
        _natural_size: &mut f32,
    ) {
        // Same rationale as `update_allocation`: needs the tree. Use
        // `update_preferred_size_with_tree`.
    }
}

impl BindConstraint {
    /// Tree-aware `update_allocation`, mirroring
    /// `clutter_bind_constraint_update_allocation`. Reads the source's
    /// position/size from `tree` and repositions/resizes `allocation`
    /// per `coordinate` + `offset`.
    pub fn update_allocation_with_tree(&self, tree: &ActorTree, allocation: &mut ActorBox) {
        let source = match self.source {
            Some(s) => s,
            None => return,
        };
        let src_x = tree.get_x(source);
        let src_y = tree.get_y(source);
        let (src_w, src_h) = tree.get_size(source);
        let actor_w = allocation.width();
        let actor_h = allocation.height();

        match self.coordinate {
            BindCoordinate::X => {
                allocation.x1 = src_x + self.offset;
                allocation.x2 = allocation.x1 + actor_w;
            }
            BindCoordinate::Y => {
                allocation.y1 = src_y + self.offset;
                allocation.y2 = allocation.y1 + actor_h;
            }
            BindCoordinate::Position => {
                allocation.x1 = src_x + self.offset;
                allocation.y1 = src_y + self.offset;
                allocation.x2 = allocation.x1 + actor_w;
                allocation.y2 = allocation.y1 + actor_h;
            }
            BindCoordinate::Width => {
                allocation.x2 = allocation.x1 + src_w + self.offset;
            }
            BindCoordinate::Height => {
                allocation.y2 = allocation.y1 + src_h + self.offset;
            }
            BindCoordinate::Size => {
                allocation.x2 = allocation.x1 + src_w + self.offset;
                allocation.y2 = allocation.y1 + src_h + self.offset;
            }
            BindCoordinate::All => {
                allocation.x1 = src_x + self.offset;
                allocation.y1 = src_y + self.offset;
                allocation.x2 = allocation.x1 + src_w + self.offset;
                allocation.y2 = allocation.y1 + src_h + self.offset;
            }
        }
    }

    /// Tree-aware `update_preferred_size`, mirroring
    /// `clutter_bind_constraint_update_preferred_size`. For width/height/
    /// size/all bindings, copies the source's preferred size into
    /// `minimum_size`/`natural_size`. The `clutter_actor_contains` guard
    /// is omitted (see module docs).
    pub fn update_preferred_size_with_tree(
        &self,
        tree: &ActorTree,
        direction: Orientation,
        for_size: f32,
        minimum_size: &mut f32,
        natural_size: &mut f32,
    ) {
        let source = match self.source {
            Some(s) => s,
            None => return,
        };
        // Only these bindings affect preferred size.
        match self.coordinate {
            BindCoordinate::Width
            | BindCoordinate::Height
            | BindCoordinate::Size
            | BindCoordinate::All => {}
            _ => return,
        }

        let for_arg = if for_size < 0.0 { None } else { Some(for_size) };
        match direction {
            Orientation::Horizontal => {
                if self.coordinate != BindCoordinate::Height {
                    let p = tree.preferred_width(source, for_arg);
                    *minimum_size = p.min;
                    *natural_size = p.natural;
                }
            }
            Orientation::Vertical => {
                if self.coordinate != BindCoordinate::Width {
                    let p = tree.preferred_height(source, for_arg);
                    *minimum_size = p.min;
                    *natural_size = p.natural;
                }
            }
        }
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

    fn make_src(tree: &mut ActorTree, x: f32, y: f32, w: f32, h: f32) -> ActorId {
        let mut cm = ActorCommon::default();
        cm.fixed_position = Some((x, y));
        let id = tree.create(cm, leaf(w, h));
        // Allocate so get_size reports the allocation, matching a real
        // source that's been laid out.
        tree.allocate(id, ActorBox::new(x, y, x + w, y + h));
        id
    }

    #[test]
    fn bind_x_offsets_allocation_x() {
        let mut tree = ActorTree::new();
        let src = make_src(&mut tree, 100.0, 50.0, 40.0, 20.0);
        let c = BindConstraint::new(Some(src), BindCoordinate::X, 5.0);
        let mut alloc = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        c.update_allocation_with_tree(&tree, &mut alloc);
        // x1 = 100 + 5 = 105, x2 = 105 + 10 = 115, y unchanged.
        assert_eq!(alloc, ActorBox::new(105.0, 0.0, 115.0, 10.0));
    }

    #[test]
    fn bind_size_sets_both_dims() {
        let mut tree = ActorTree::new();
        let src = make_src(&mut tree, 0.0, 0.0, 40.0, 20.0);
        let c = BindConstraint::new(Some(src), BindCoordinate::Size, 5.0);
        let mut alloc = ActorBox::new(10.0, 10.0, 100.0, 100.0);
        c.update_allocation_with_tree(&tree, &mut alloc);
        // x2 = x1 + 40 + 5 = 55, y2 = y1 + 20 + 5 = 35
        assert_eq!(alloc, ActorBox::new(10.0, 10.0, 55.0, 35.0));
    }

    #[test]
    fn bind_width_updates_preferred_width_from_source() {
        let mut tree = ActorTree::new();
        let src = tree.create(ActorCommon::default(), leaf(80.0, 30.0));
        let c = BindConstraint::new(Some(src), BindCoordinate::Width, 0.0);
        let mut min = 0.0_f32;
        let mut nat = 0.0_f32;
        c.update_preferred_size_with_tree(&tree, Orientation::Horizontal, -1.0, &mut min, &mut nat);
        assert_eq!((min, nat), (0.0, 80.0));
    }

    #[test]
    fn bind_y_does_not_affect_preferred_width() {
        let mut tree = ActorTree::new();
        let src = tree.create(ActorCommon::default(), leaf(80.0, 30.0));
        let c = BindConstraint::new(Some(src), BindCoordinate::Y, 0.0);
        let mut min = 99.0_f32;
        let mut nat = 99.0_f32;
        c.update_preferred_size_with_tree(&tree, Orientation::Horizontal, -1.0, &mut min, &mut nat);
        // Y binding doesn't touch horizontal preferred size.
        assert_eq!((min, nat), (99.0, 99.0));
    }
}
