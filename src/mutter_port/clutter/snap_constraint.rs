//! Port of `clutter/clutter-snap-constraint.{c,h}`.
//!
//! `ClutterSnapConstraint` snaps one of the constrained actor's edges to
//! an edge of a `source` actor (plus a pixel `offset`), e.g. "my left
//! edge sits at the source's right edge + 10px".
//!
//! Ported: `update_allocation`'s edge-snapping math
//! (`clutter_snap_constraint_update_allocation`) for all four
//! `from_edge`/`to_edge` combinations.
//!
//! Skipped (GObject machinery, no Rust equivalent needed): property
//! install/notify (`PROP_SOURCE`/`PROP_FROM_EDGE`/`PROP_TO_EDGE`/
//! `PROP_OFFSET`), `dispose`/weak-pointer bookkeeping on the source
//! actor, and the `set_actor` override that reruns allocation on
//! attach (upstream's `clutter_actor_meta_set_actor` queues a
//! relayout; this port has no relayout queue yet, matching the note in
//! `align_constraint.rs`).

use super::actor::{ActorId, ActorTree};
use super::actor_box::ActorBox;
use super::actor_meta::ActorMeta;
use super::constraint::Constraint;
use super::enums::SnapEdge;

/// `ClutterSnapConstraint`. `meta` (the `ClutterActorMeta` base: name,
/// enabled, priority) is held separately by the caller, matching the
/// convention established in `align_constraint.rs`/`bind_constraint.rs`.
#[derive(Debug, Clone, Default)]
pub struct SnapConstraint {
    source: Option<ActorId>,
    from_edge: SnapEdge,
    to_edge: SnapEdge,
    offset: f32,
}

impl SnapConstraint {
    pub fn new(
        source: Option<ActorId>,
        from_edge: SnapEdge,
        to_edge: SnapEdge,
        offset: f32,
    ) -> Self {
        SnapConstraint {
            source,
            from_edge,
            to_edge,
            offset,
        }
    }

    pub fn set_source(&mut self, source: Option<ActorId>) {
        self.source = source;
    }

    pub fn source(&self) -> Option<ActorId> {
        self.source
    }

    pub fn set_edges(&mut self, from_edge: SnapEdge, to_edge: SnapEdge) {
        self.from_edge = from_edge;
        self.to_edge = to_edge;
    }

    pub fn edges(&self) -> (SnapEdge, SnapEdge) {
        (self.from_edge, self.to_edge)
    }

    pub fn set_offset(&mut self, offset: f32) {
        self.offset = offset;
    }

    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// `clutter_snap_constraint_update_allocation`: reads `to_edge`'s
    /// coordinate off the source actor's current allocation, applies
    /// `offset`, and writes it into `from_edge` of `allocation`
    /// (translating the whole box so its size is preserved).
    ///
    /// A missing `source` is a no-op, matching upstream (`if (source ==
    /// NULL) return;`).
    pub fn update_allocation_with_tree(&self, tree: &ActorTree, allocation: &mut ActorBox) {
        let source = match self.source {
            Some(s) => s,
            None => return,
        };
        let source_alloc = tree.get_allocation(source);

        let source_coord = match self.to_edge {
            SnapEdge::Top => source_alloc.y1,
            SnapEdge::Right => source_alloc.x2,
            SnapEdge::Bottom => source_alloc.y2,
            SnapEdge::Left => source_alloc.x1,
        };
        let target = source_coord + self.offset;

        let width = allocation.width();
        let height = allocation.height();

        match self.from_edge {
            SnapEdge::Left => {
                allocation.x1 = target;
                allocation.x2 = target + width;
            }
            SnapEdge::Right => {
                allocation.x2 = target;
                allocation.x1 = target - width;
            }
            SnapEdge::Top => {
                allocation.y1 = target;
                allocation.y2 = target + height;
            }
            SnapEdge::Bottom => {
                allocation.y2 = target;
                allocation.y1 = target - height;
            }
        }
    }
}

impl Constraint for SnapConstraint {
    fn update_allocation(
        &mut self,
        _meta: &mut ActorMeta,
        _actor: ActorId,
        _allocation: &mut ActorBox,
    ) {
        // No-op: needs `&ActorTree` to read the source's allocation,
        // which the `Constraint` trait's signature doesn't carry (same
        // constraint noted in `align_constraint.rs`/`bind_constraint.rs`
        // — callers must use `update_allocation_with_tree` directly).
    }
}

#[cfg(test)]
mod tests {
    use super::super::actor::{ActorCommon, NullBehavior};
    use super::*;
    use alloc::boxed::Box;

    fn make_actor(tree: &mut ActorTree, alloc: ActorBox) -> ActorId {
        let id = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        tree.allocate(id, alloc);
        id
    }

    #[test]
    fn left_snaps_to_source_right_plus_offset() {
        let mut tree = ActorTree::new();
        let source = make_actor(&mut tree, ActorBox::new(0.0, 0.0, 50.0, 50.0));
        let c = SnapConstraint::new(Some(source), SnapEdge::Left, SnapEdge::Right, 10.0);

        let mut alloc = ActorBox::new(0.0, 0.0, 20.0, 20.0);
        c.update_allocation_with_tree(&tree, &mut alloc);
        // source right edge = 50, + offset 10 = 60; width preserved (20).
        assert_eq!(alloc, ActorBox::new(60.0, 0.0, 80.0, 20.0));
    }

    #[test]
    fn right_snaps_to_source_left_minus_offset() {
        let mut tree = ActorTree::new();
        let source = make_actor(&mut tree, ActorBox::new(100.0, 0.0, 150.0, 50.0));
        let c = SnapConstraint::new(Some(source), SnapEdge::Right, SnapEdge::Left, -10.0);

        let mut alloc = ActorBox::new(0.0, 0.0, 20.0, 20.0);
        c.update_allocation_with_tree(&tree, &mut alloc);
        // source left edge = 100, + offset (-10) = 90; x1 = 90 - width(20) = 70.
        assert_eq!(alloc, ActorBox::new(70.0, 0.0, 90.0, 20.0));
    }

    #[test]
    fn top_and_bottom_snap_vertically() {
        let mut tree = ActorTree::new();
        let source = make_actor(&mut tree, ActorBox::new(0.0, 0.0, 50.0, 50.0));
        let c = SnapConstraint::new(Some(source), SnapEdge::Top, SnapEdge::Bottom, 5.0);

        let mut alloc = ActorBox::new(0.0, 0.0, 10.0, 30.0);
        c.update_allocation_with_tree(&tree, &mut alloc);
        assert_eq!(alloc, ActorBox::new(0.0, 55.0, 10.0, 85.0));
    }

    #[test]
    fn missing_source_is_noop() {
        let tree = ActorTree::new();
        let c = SnapConstraint::new(None, SnapEdge::Left, SnapEdge::Right, 10.0);
        let mut alloc = ActorBox::new(1.0, 2.0, 3.0, 4.0);
        c.update_allocation_with_tree(&tree, &mut alloc);
        assert_eq!(alloc, ActorBox::new(1.0, 2.0, 3.0, 4.0));
    }
}
