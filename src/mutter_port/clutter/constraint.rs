//! Port of GNOME mutter's `clutter/clutter-constraint.{c,h}` and
//! `clutter-constraint-private.h`.
//!
//! `ClutterConstraint` is the abstract base class for modifiers of an
//! actor's position or size. It extends `ClutterActorMeta` and adds two
//! virtuals: `update_allocation` (modify the actor's allocation box before
//! its own `allocate` runs) and `update_preferred_size` (modify the
//! actor's min/natural size request).
//!
//! # What's ported
//!
//! - The `ClutterConstraintClass` vtable as a `Constraint` trait extending
//!   the ported `ActorMeta` storage. The two virtuals
//!   (`update_allocation`, `update_preferred_size`) default to no-ops,
//!   matching the C `constraint_update_*` stubs.
//! - `clutter_constraint_update_allocation` (the wrapper that calls the
//!   virtual and returns whether the box changed) as
//!   `Constraint::apply_update_allocation`.
//! - `clutter_constraint_update_preferred_size` (the wrapper that calls the
//!   virtual) as `Constraint::apply_update_preferred_size`.
//! - The `set_enabled` override: in C, enabling/disabling a constraint
//!   queues a relayout on the attached actor (`clutter_actor_queue_relayout`).
//!   That queue API isn't on `ActorTree` yet, so the ported `set_enabled`
//!   returns whether a relayout would be needed (the caller queues it);
//!   the field is still updated.
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_ABSTRACT_TYPE`, `ClutterActorMeta`
//!   parent chaining, `GParamSpec`): no GObject in this port. The
//!   constraint's `ActorMeta` storage is just embedded.
//! - `clutter_actor_queue_relayout` on `set_enabled`: not ported on
//!   `ActorTree` yet; the ported `set_enabled` returns the would-queue
//!   signal instead.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use super::actor::ActorId;
use super::actor_box::ActorBox;
use super::actor_meta::ActorMeta;
use super::enums::Orientation;

/// Port of `ClutterConstraintClass` vtable. Implement this per constraint
/// type instead of subclassing the GObject. The `ActorMeta` storage (actor,
/// name, enabled, priority) is held separately and passed in, matching how
/// the C `ClutterConstraint` wraps `ClutterActorMeta`.
pub trait Constraint {
    /// `ClutterConstraintClass::update_allocation`: modify `allocation` in
    /// place. Default no-op (matching `constraint_update_allocation`).
    fn update_allocation(
        &mut self,
        _meta: &mut ActorMeta,
        _actor: ActorId,
        _allocation: &mut ActorBox,
    ) {
    }

    /// `ClutterConstraintClass::update_preferred_size`: modify
    /// `minimum_size`/`natural_size` in place for the given `direction`
    /// (`for_size` is the size in the opposite direction, or `< 0.0` for
    /// unconstrained). Default no-op (matching
    /// `constraint_update_preferred_size`).
    fn update_preferred_size(
        &mut self,
        _meta: &mut ActorMeta,
        _actor: ActorId,
        _direction: Orientation,
        _for_size: f32,
        _minimum_size: &mut f32,
        _natural_size: &mut f32,
    ) {
    }
}

/// Port of `clutter_constraint_update_allocation`: invoke the virtual and
/// return whether the allocation changed (matching the C `gboolean` return
/// based on `!clutter_actor_box_equal`).
pub fn apply_update_allocation<C: Constraint + ?Sized>(
    constraint: &mut C,
    meta: &mut ActorMeta,
    actor: ActorId,
    allocation: &mut ActorBox,
) -> bool {
    let old = *allocation;
    constraint.update_allocation(meta, actor, allocation);
    *allocation != old
}

/// Port of `clutter_constraint_update_preferred_size`: invoke the virtual,
/// mutating the given min/natural sizes.
pub fn apply_update_preferred_size<C: Constraint + ?Sized>(
    constraint: &mut C,
    meta: &mut ActorMeta,
    actor: ActorId,
    direction: Orientation,
    for_size: f32,
    minimum_size: &mut f32,
    natural_size: &mut f32,
) {
    constraint.update_preferred_size(meta, actor, direction, for_size, minimum_size, natural_size);
}

/// Port of the `ClutterConstraint::set_enabled` override. In C this calls
/// `clutter_actor_queue_relayout` on the attached actor before chaining up
/// to the parent `set_enabled`. Since `ActorTree` has no relayout queue
/// yet, this updates the `enabled` field and returns whether a relayout
/// would be queued (i.e. the actor is attached and the flag changed), so
/// the caller can perform the queue.
///
/// Returns `true` if a relayout should be queued on `meta.actor`, `false`
/// otherwise.
pub fn set_enabled(meta: &mut ActorMeta, enabled: bool) -> bool {
    let changed = meta.enabled != enabled;
    let attached = meta.actor.is_some();
    meta.enabled = enabled;
    changed && attached
}

#[cfg(test)]
mod tests {
    use super::super::actor::{ActorCommon, ActorTree, NullBehavior};
    use super::super::actor_box::ActorBox;
    use super::*;
    use alloc::boxed::Box;

    /// A constraint that offsets the allocation by (10, 20).
    struct Offset;
    impl Constraint for Offset {
        fn update_allocation(
            &mut self,
            _meta: &mut ActorMeta,
            _actor: ActorId,
            allocation: &mut ActorBox,
        ) {
            allocation.x1 += 10.0;
            allocation.y1 += 20.0;
            allocation.x2 += 10.0;
            allocation.y2 += 20.0;
        }
    }

    #[test]
    fn apply_update_allocation_reports_change() {
        let mut tree = ActorTree::new();
        let id = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let mut meta = ActorMeta::new();
        meta.set_actor(Some(id));
        let mut c = Offset;
        let mut box_ = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        assert!(apply_update_allocation(&mut c, &mut meta, id, &mut box_));
        assert_eq!(box_, ActorBox::new(10.0, 20.0, 20.0, 30.0));
    }

    /// A constraint that does nothing (exercises the default virtuals).
    struct Noop;
    impl Constraint for Noop {}

    #[test]
    fn apply_update_allocation_noop_reports_no_change() {
        let mut tree = ActorTree::new();
        let id = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let mut meta = ActorMeta::new();
        meta.set_actor(Some(id));
        let mut c = Noop;
        let mut box_ = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        assert!(!apply_update_allocation(&mut c, &mut meta, id, &mut box_));
        assert_eq!(box_, ActorBox::new(0.0, 0.0, 10.0, 10.0));
    }

    #[test]
    fn set_enabled_returns_relayout_signal() {
        let mut tree = ActorTree::new();
        let id = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let mut meta = ActorMeta::new();
        // Not attached -> no relayout signal.
        assert!(!set_enabled(&mut meta, false));
        meta.set_actor(Some(id));
        // Attached + changed -> signal.
        assert!(set_enabled(&mut meta, true));
        // Attached + unchanged -> no signal.
        assert!(!set_enabled(&mut meta, true));
    }
}
