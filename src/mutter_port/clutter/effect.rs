//! Port of GNOME mutter's `clutter/clutter-effect.{c,h}` and
//! `clutter-effect-private.h`.
//!
//! `ClutterEffect` is the abstract base class for paint-time modifiers
//! attached to an actor (offscreen redirect, deformed paint, shader
//! passes, ...). It extends `ClutterActorMeta` and adds a vtable of paint
//! virtuals: `pre_paint`, `post_paint`, `modify_paint_volume` (called
//! `paint_volume` upstream but the C vtable entry is
//! `modify_paint_volume`), `get_paint_volume`, and `pick.
//!
//! # What's ported
//!
//! - The `ClutterEffectClass` vtable as an `Effect` trait extending the
//!   ported `ActorMeta` storage. The five virtuals (`pre_paint`,
//!   `post_paint`, `modify_paint_volume`, `get_paint_volume`, `pick`)
//!   default to no-ops / `false`, matching the C `effect_*` stubs.
//! - `clutter_effect_queue_repaint` — marks the effect's actor as needing
//!   a redraw. In C this calls `clutter_actor_queue_redraw` on the
//!   attached actor; that queue API isn't on `ActorTree` yet, so the
//!   ported helper returns whether a repaint would be queued (the caller
//!   performs it), matching the convention used in `constraint.rs`.
//! - The `run_*` wrappers (`clutter_effect_pre_paint`,
//!   `_clutter_effect_post_paint`, `_clutter_effect_modify_paint_volume`,
//!   `_clutter_effect_get_paint_volume`, `_clutter_effect_pick`) that
//!   invoke the virtual and return its `gboolean` result.
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_ABSTRACT_TYPE`, `ClutterActorMeta`
//!   parent chaining, `GParamSpec`): no GObject in this port.
//! - `clutter_actor_queue_redraw` / `_clutter_actor_queue_only_relayout`
//!   on `queue_repaint` / `set_enabled`: not on `ActorTree` yet; the
//!   helpers return the would-queue signal instead.
//! - The `ClutterPaintVolume` type: `modify_paint_volume` /
//!   `get_paint_volume` take a `&mut PaintVolume` / `&PaintVolume` but
//!   `PaintVolume` isn't ported yet (it's a 3D volume used by the culling
//!   pass). The trait methods take `&mut ()` / `&()` as opaque stand-ins
//!   and default to no-ops; a real `PaintVolume` type can replace the
//!   `()` once ported without changing the trait shape callers depend on.
//! - The `pick` virtual's `ClutterPaintNode *` argument: represented as
//!   `&mut PaintNode` (already ported in `paint_node.rs`).
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use super::actor::ActorId;
use super::actor_meta::ActorMeta;
use super::enums::EffectPaintFlags;
use super::paint_context::PaintContext;
use super::paint_node::PaintNode;

/// Opaque stand-in for `ClutterPaintVolume` until that type is ported.
/// See module docs.
pub type PaintVolumeRef<'a> = &'a mut ();
pub type PaintVolumeConstRef<'a> = &'a ();

/// Port of `ClutterEffectClass` vtable. Implement this per effect type
/// instead of subclassing the GObject. The `ActorMeta` storage (actor,
/// name, enabled, priority) is held separately and passed in.
pub trait Effect {
    /// `ClutterEffectClass::pre_paint`: run before the actor's own paint.
    /// Return `true` to signal the effect took over painting (matching the
    /// C `gboolean` return; `false` lets the actor paint normally).
    /// Default `false` (matching `effect_real_pre_paint`).
    fn pre_paint(
        &mut self,
        _meta: &mut ActorMeta,
        _actor: ActorId,
        _ctx: &PaintContext,
        _flags: EffectPaintFlags,
    ) -> bool {
        false
    }

    /// `ClutterEffectClass::post_paint`: run after the actor's own paint.
    /// Default no-op (matching `effect_real_post_paint`).
    fn post_paint(
        &mut self,
        _meta: &mut ActorMeta,
        _actor: ActorId,
        _ctx: &PaintContext,
        _flags: EffectPaintFlags,
    ) {
    }

    /// `ClutterEffectClass::modify_paint_volume`: mutate the effect's
    /// paint volume. Return `true` if modified (matching the C `gboolean`).
    /// Default `false` (matching `effect_real_modify_paint_volume`).
    fn modify_paint_volume(
        &mut self,
        _meta: &mut ActorMeta,
        _actor: ActorId,
        _volume: PaintVolumeRef<'_>,
    ) -> bool {
        false
    }

    /// `ClutterEffectClass::get_paint_volume`: report the effect's paint
    /// volume. Return `true` if a volume was provided (matching the C
    /// `gboolean`). Default `false` (matching `effect_real_get_paint_volume`).
    fn get_paint_volume(
        &mut self,
        _meta: &mut ActorMeta,
        _actor: ActorId,
        _volume: PaintVolumeConstRef<'_>,
    ) -> bool {
        false
    }

    /// `ClutterEffectClass::pick`: produce the effect's contribution to
    /// the pick (color-code) pass. Return `true` if the effect took over
    /// picking. Default `false` (matching `effect_real_pick`).
    fn pick(
        &mut self,
        _meta: &mut ActorMeta,
        _actor: ActorId,
        _node: &mut PaintNode,
        _ctx: &PaintContext,
    ) -> bool {
        false
    }
}

/// `_clutter_effect_pre_paint`: invoke the virtual and return its result.
pub fn run_pre_paint<E: Effect + ?Sized>(
    effect: &mut E,
    meta: &mut ActorMeta,
    actor: ActorId,
    ctx: &PaintContext,
    flags: EffectPaintFlags,
) -> bool {
    effect.pre_paint(meta, actor, ctx, flags)
}

/// `_clutter_effect_post_paint`: invoke the virtual.
pub fn run_post_paint<E: Effect + ?Sized>(
    effect: &mut E,
    meta: &mut ActorMeta,
    actor: ActorId,
    ctx: &PaintContext,
    flags: EffectPaintFlags,
) {
    effect.post_paint(meta, actor, ctx, flags);
}

/// `_clutter_effect_modify_paint_volume`: invoke the virtual.
pub fn run_modify_paint_volume<E: Effect + ?Sized>(
    effect: &mut E,
    meta: &mut ActorMeta,
    actor: ActorId,
    volume: PaintVolumeRef<'_>,
) -> bool {
    effect.modify_paint_volume(meta, actor, volume)
}

/// `_clutter_effect_get_paint_volume`: invoke the virtual.
pub fn run_get_paint_volume<E: Effect + ?Sized>(
    effect: &mut E,
    meta: &mut ActorMeta,
    actor: ActorId,
    volume: PaintVolumeConstRef<'_>,
) -> bool {
    effect.get_paint_volume(meta, actor, volume)
}

/// `_clutter_effect_pick`: invoke the virtual.
pub fn run_pick<E: Effect + ?Sized>(
    effect: &mut E,
    meta: &mut ActorMeta,
    actor: ActorId,
    node: &mut PaintNode,
    ctx: &PaintContext,
) -> bool {
    effect.pick(meta, actor, node, ctx)
}

/// `clutter_effect_queue_repaint`: in C this calls
/// `clutter_actor_queue_redraw` on the attached actor. Since that queue
/// API isn't on `ActorTree` yet, this returns whether a repaint would be
/// queued (i.e. the actor is attached), so the caller can perform it.
///
/// Returns `true` if a repaint should be queued on `meta.actor`, `false`
/// otherwise.
pub fn queue_repaint(meta: &ActorMeta) -> bool {
    meta.actor.is_some()
}

/// Port of the `ClutterEffect::set_enabled` override. In C this calls
/// `clutter_actor_queue_redraw` on the attached actor before chaining up
/// to the parent `set_enabled`. Since `ActorTree` has no redraw queue
/// yet, this updates the `enabled` field and returns whether a redraw
/// would be queued (the caller performs it), matching `constraint::set_enabled`.
pub fn set_enabled(meta: &mut ActorMeta, enabled: bool) -> bool {
    let changed = meta.enabled != enabled;
    let attached = meta.actor.is_some();
    meta.enabled = enabled;
    changed && attached
}

#[cfg(test)]
mod tests {
    use super::super::actor::{ActorCommon, ActorTree, NullBehavior};
    use super::super::paint_context::{Framebuffer, PaintFlag};
    use super::*;
    use alloc::boxed::Box;

    /// An effect that takes over painting.
    struct TakeOver;
    impl Effect for TakeOver {
        fn pre_paint(
            &mut self,
            _meta: &mut ActorMeta,
            _actor: ActorId,
            _ctx: &PaintContext,
            _flags: EffectPaintFlags,
        ) -> bool {
            true
        }
    }

    fn ctx() -> PaintContext {
        PaintContext::new_for_framebuffer(
            Framebuffer,
            None,
            PaintFlag::NONE,
            super::super::paint_context::ColorState,
        )
    }

    #[test]
    fn default_pre_paint_returns_false() {
        struct Noop;
        impl Effect for Noop {}
        let mut e = Noop;
        let mut meta = ActorMeta::new();
        let mut tree = ActorTree::new();
        let id = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        assert!(!run_pre_paint(
            &mut e,
            &mut meta,
            id,
            &ctx(),
            EffectPaintFlags::NONE
        ));
    }

    #[test]
    fn takeover_pre_paint_returns_true() {
        let mut e = TakeOver;
        let mut meta = ActorMeta::new();
        let mut tree = ActorTree::new();
        let id = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        assert!(run_pre_paint(
            &mut e,
            &mut meta,
            id,
            &ctx(),
            EffectPaintFlags::NONE
        ));
    }

    #[test]
    fn queue_repaint_signals_only_when_attached() {
        let mut meta = ActorMeta::new();
        assert!(!queue_repaint(&meta)); // not attached
        let mut tree = ActorTree::new();
        let id = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        meta.set_actor(Some(id));
        assert!(queue_repaint(&meta));
    }

    #[test]
    fn set_enabled_returns_redraw_signal() {
        let mut tree = ActorTree::new();
        let id = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let mut meta = ActorMeta::new();
        assert!(!set_enabled(&mut meta, false)); // not attached
        meta.set_actor(Some(id));
        assert!(set_enabled(&mut meta, true)); // attached + changed
        assert!(!set_enabled(&mut meta, true)); // attached + unchanged
    }
}
