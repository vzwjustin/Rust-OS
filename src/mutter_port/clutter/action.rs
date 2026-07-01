//! Port of GNOME mutter's `clutter/clutter-action.{c,h}` and
//! `clutter-action-private.h`.
//!
//! `ClutterAction` is the abstract base class for event-handling modifiers
//! attached to an actor (drag-and-drop, click gestures, panning, ...). It
//! extends `ClutterActorMeta` and adds a `handle_event` virtual plus
//! sequence-management virtuals, plus a `phase` field controlling when the
//! action sees events relative to the target actor (capture / target /
//! bubble).
//!
//! # What's ported
//!
//! - The `ClutterActionClass` vtable as an `Action` trait extending the
//!   ported `ActorMeta` storage. The four virtuals (`handle_event`,
//!   `sequence_cancelled`, `register_sequence`, `setup_sequence_relationship`)
//!   default to no-ops / `false` / `true` / `0`, matching the C
//!   `clutter_action_handle_event_default` and the null-check guards in the
//!   wrapper functions.
//! - The `ClutterActionPrivate::phase` field as a plain `EventPhase` on the
//!   trait, with `set_phase`/`get_phase` accessors matching
//!   `clutter_action_set_phase`/`_get_phase`.
//! - `clutter_action_handle_event`: the wrapper that checks the meta's actor
//!   is set before dispatching (matching the C
//!   `clutter_actor_meta_get_actor` guard), returning whether the event was
//!   consumed (`CLUTTER_EVENT_STOP` == `true`).
//! - `clutter_action_sequence_cancelled` /
//!   `_register_sequence` / `_setup_sequence_relationship` wrappers, each
//!   guarding on the virtual being present (the C null-checks become the
//!   trait's default impls).
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_ABSTRACT_TYPE_WITH_PRIVATE`,
//!   `ClutterActorMeta` parent chaining, `GParamSpec`): no GObject in this
//!   port. The `phase` field is plain.
//! - `ClutterSprite *` in `sequence_cancelled`/`setup_sequence_relationship`:
//!   `ClutterSprite` isn't ported (it's a touch-sequence tracking type);
//!   these virtuals take a `u32` sprite-id placeholder so the trait shape is
//!   stable when `Sprite` lands.
//! - The `clutter_actor_add_action`/`_remove_action`/`_get_actions`/
//!   `_clear_actions` API: these live on `ClutterActor`, not
//!   `ClutterAction`. They'd be ported as `ActorTree` methods that store
//!   `Box<dyn Action>` per actor; that storage isn't on `ActorTree` yet, so
//!   these are deferred to the actor-actions-storage wave. The `Action`
//!   trait itself is ready for that wiring.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use super::actor_meta::ActorMeta;
use super::enums::EventPhase;
use super::event::Event;

/// Port of `ClutterActionClass` vtable. Implement this per action type
/// instead of subclassing the GObject. The `ActorMeta` storage (actor,
/// name, enabled, priority) is held separately and passed in.
///
/// The `phase` field is stored on the implementing struct (or on a wrapper)
/// since it's per-instance state, not per-class — see `ActionState` below
/// for a helper that bundles it.
pub trait Action {
    /// `ClutterActionClass::handle_event`: return `true` to stop further
    /// propagation (`CLUTTER_EVENT_STOP`), `false` to propagate
    /// (`CLUTTER_EVENT_PROPAGATE`). Default `false` (matching
    /// `clutter_action_handle_event_default`).
    fn handle_event(&mut self, _meta: &mut ActorMeta, _event: &Event) -> bool {
        false
    }

    /// `ClutterActionClass::sequence_cancelled`. Default no-op (matching
    /// the C null-check guard skipping the call when the virtual is unset).
    fn sequence_cancelled(&mut self, _sprite: u32) {}

    /// `ClutterActionClass::register_sequence`: return `true` on success
    /// (matching the C default `return TRUE` when the virtual is unset).
    fn register_sequence(&mut self, _event: &Event) -> bool {
        true
    }

    /// `ClutterActionClass::setup_sequence_relationship`: return a status
    /// int (matching the C default `return 0` when the virtual is unset).
    fn setup_sequence_relationship(&mut self, _other: &mut dyn Action, _sprite: u32) -> i32 {
        0
    }
}

/// Per-instance `phase` storage, equivalent to
/// `ClutterActionPrivate::phase`. The C version stores this in the
/// action's private struct; here it's a separate field the implementing
/// struct holds (or uses this helper for).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ActionState {
    pub phase: EventPhase,
}

impl ActionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// `clutter_action_set_phase`.
    pub fn set_phase(&mut self, phase: EventPhase) {
        self.phase = phase;
    }

    /// `clutter_action_get_phase`.
    pub fn phase(&self) -> EventPhase {
        self.phase
    }
}

/// `clutter_action_handle_event`: invoke the virtual only if the meta's
/// actor is set (matching the C `clutter_actor_meta_get_actor` guard),
/// returning whether the event was consumed.
pub fn handle_event<A: Action + ?Sized>(
    action: &mut A,
    meta: &mut ActorMeta,
    event: &Event,
) -> bool {
    if meta.actor.is_none() {
        return false;
    }
    action.handle_event(meta, event)
}

/// `clutter_action_sequence_cancelled`: invoke the virtual.
pub fn sequence_cancelled<A: Action + ?Sized>(action: &mut A, sprite: u32) {
    action.sequence_cancelled(sprite);
}

/// `clutter_action_register_sequence`: invoke the virtual, defaulting to
/// `true` when unset (the trait's default impl).
pub fn register_sequence<A: Action + ?Sized>(action: &mut A, event: &Event) -> bool {
    action.register_sequence(event)
}

/// `clutter_action_setup_sequence_relationship`: invoke the virtual,
/// defaulting to `0` when unset.
pub fn setup_sequence_relationship<A: Action + ?Sized>(
    action_1: &mut A,
    action_2: &mut dyn Action,
    sprite: u32,
) -> i32 {
    action_1.setup_sequence_relationship(action_2, sprite)
}

#[cfg(test)]
mod tests {
    use super::super::actor::{ActorCommon, ActorTree, NullBehavior};
    use super::super::event::{ButtonEvent, DeviceId, EventFlags, ModifierType};
    use super::*;
    use alloc::boxed::Box;

    /// An action that consumes button-press events.
    struct ConsumePress;
    impl Action for ConsumePress {
        fn handle_event(&mut self, _meta: &mut ActorMeta, event: &Event) -> bool {
            matches!(event, Event::Button(b) if b.button != 0)
        }
    }

    fn press_event() -> Event {
        Event::Button(ButtonEvent {
            time_us: 0,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(0)),
            x: 0.0,
            y: 0.0,
            modifier_state: ModifierType::NONE,
            button: 1,
            tool: None,
            evdev_code: 0,
        })
    }

    #[test]
    fn default_handle_event_propagates() {
        struct Noop;
        impl Action for Noop {}
        let mut a = Noop;
        let mut meta = ActorMeta::new();
        meta.set_actor(Some(
            ActorTree::new().create(ActorCommon::default(), Box::new(NullBehavior::default())),
        ));
        assert!(!handle_event(&mut a, &mut meta, &press_event()));
    }

    #[test]
    fn handle_event_guarded_by_actor_set() {
        let mut a = ConsumePress;
        let mut meta = ActorMeta::new();
        // No actor set -> guard returns false even though the action would
        // consume.
        assert!(!handle_event(&mut a, &mut meta, &press_event()));
        // With actor set -> action consumes.
        meta.set_actor(Some(
            ActorTree::new().create(ActorCommon::default(), Box::new(NullBehavior::default())),
        ));
        assert!(handle_event(&mut a, &mut meta, &press_event()));
    }

    #[test]
    fn phase_round_trips() {
        let mut st = ActionState::new();
        assert_eq!(st.phase(), EventPhase::Capture); // default
        st.set_phase(EventPhase::Bubble);
        assert_eq!(st.phase(), EventPhase::Bubble);
    }

    #[test]
    fn register_sequence_defaults_true() {
        struct Noop;
        impl Action for Noop {}
        let mut a = Noop;
        assert!(register_sequence(&mut a, &press_event()));
    }

    #[test]
    fn setup_sequence_relationship_defaults_zero() {
        struct Noop;
        impl Action for Noop {}
        let mut a = Noop;
        let mut b = Noop;
        assert_eq!(setup_sequence_relationship(&mut a, &mut b, 0), 0);
    }
}
