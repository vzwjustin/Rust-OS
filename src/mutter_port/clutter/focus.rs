//! Port of GNOME mutter's `clutter/clutter-focus.{c,h}` and
//! `clutter-focus-private.h`.
//!
//! `ClutterFocus` is the abstract base class for keyboard focus
//! management: it tracks the currently focused actor, propagates
//! keyboard events to the focus chain, and is notified when a grab
//! changes the effective focus target. Subclasses (`ClutterKeyFocus`,
//! `ClutterClickFocus`) provide concrete focus policies.
//!
//! # What's ported
//!
//! - The `ClutterFocusClass` vtable as a `Focus` trait with four
//!   virtuals: `set_current_actor` (returns whether the focus moved),
//!   `get_current_actor`, `propagate_event`, `update_from_event`, and
//!   `notify_grab`. Default implementations match the C null-vtable
//!   guards (no-op for `propagate_event`/`update_from_event`/`notify_grab`,
//!   `false`/`None` for the actor accessors).
//! - `clutter_focus_set_current_actor` / `_get_current_actor` /
//!   `_propagate_event` / `_update_from_event` / `_notify_grab` wrapper
//!   functions, each dispatching through the trait.
//! - `clutter_focus_get_stage`: dropped (no `Stage` port); the C
//!   `ClutterFocusPrivate::stage` field is a construct-only back-pointer
//!   that the focus impl uses to access the stage. A future `Stage` port
//!   can add it.
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_TYPE_WITH_PRIVATE`, `GParamSpec` for
//!   the `stage` property, `constructed`/`finalize`/`set_property`/
//!   `get_property`): plain trait + storage. The C `finalize` calls
//!   `set_current_actor(focus, NULL, NULL, CLUTTER_CURRENT_TIME)` to
//!   clear focus on destruction; a `Drop` impl on the concrete subclass
//!   can do the same.
//! - `ClutterStage *stage`: no `Stage` port. The stage back-pointer is
//!   used by subclasses to access the actor tree; the trait is
//!   stage-agnostic and a subclass port can hold the stage separately.
//! - `CLUTTER_CURRENT_TIME` (a `0` sentinel for "current time"): passed
//!   through as a plain `u32`; callers use `0` for the current-time
//!   sentinel.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use super::event::{DeviceId, Event};

/// `CLUTTER_CURRENT_TIME` — the sentinel "current time" value (0) used
/// by `clutter_focus_set_current_actor` when the caller doesn't have a
/// specific timestamp.
pub const CURRENT_TIME: u32 = 0;

/// Port of `ClutterFocusClass` vtable. Implement this per focus policy
/// (key focus, click focus) instead of subclassing the GObject.
///
/// Generic over the actor-id type (`Id`) so it can be used with both
/// `mutter_port::clutter::actor::ActorId` and
/// `desktop::window_manager::WindowId` without coupling.
///
/// The `set_current_actor` virtual returns `true` if the focus moved
/// (matching the C `gboolean` return); `get_current_actor` returns the
/// focused actor or `None`.
pub trait Focus<Id: Copy + Eq + PartialEq> {
    /// `ClutterFocusClass::set_current_actor`: set the focused actor.
    /// Returns `true` if the focus changed. `source_device` is the
    /// device that triggered the focus change (or `None`); `time_ms` is
    /// the event timestamp (or `CURRENT_TIME`).
    fn set_current_actor(
        &mut self,
        actor: Option<Id>,
        source_device: Option<DeviceId>,
        time_ms: u32,
    ) -> bool;

    /// `ClutterFocusClass::get_current_actor`: return the currently
    /// focused actor, or `None`.
    fn current_actor(&self) -> Option<Id>;

    /// `ClutterFocusClass::propagate_event`: dispatch a keyboard event
    /// through the focus chain. Default no-op (matching the C
    /// null-vtable guard).
    fn propagate_event(&mut self, _event: &Event) {}

    /// `ClutterFocusClass::update_from_event`: update the focus based on
    /// an event (e.g. a button press focusing the clicked actor).
    /// Default no-op (matching the C `if (focus_class->update_from_event)`
    /// null-check).
    fn update_from_event(&mut self, _event: &Event) {}

    /// `ClutterFocusClass::notify_grab`: called when a grab changes the
    /// effective focus target. `grab_actor` is the new grab's actor,
    /// `old_grab_actor` is the previous grab's actor. Default no-op.
    fn notify_grab(&mut self, grab_actor: Option<Id>, old_grab_actor: Option<Id>) {
        let _ = (grab_actor, old_grab_actor);
    }
}

// ---- wrapper functions matching the C `clutter_focus_*` API ----

/// `clutter_focus_set_current_actor`.
pub fn set_current_actor<F: Focus<Id> + ?Sized, Id: Copy + Eq + PartialEq>(
    focus: &mut F,
    actor: Option<Id>,
    source_device: Option<DeviceId>,
    time_ms: u32,
) -> bool {
    focus.set_current_actor(actor, source_device, time_ms)
}

/// `clutter_focus_get_current_actor`.
pub fn current_actor<F: Focus<Id> + ?Sized, Id: Copy + Eq + PartialEq>(focus: &F) -> Option<Id> {
    focus.current_actor()
}

/// `clutter_focus_propagate_event`.
pub fn propagate_event<F: Focus<Id> + ?Sized, Id: Copy + Eq + PartialEq>(
    focus: &mut F,
    event: &Event,
) {
    focus.propagate_event(event);
}

/// `clutter_focus_update_from_event`.
pub fn update_from_event<F: Focus<Id> + ?Sized, Id: Copy + Eq + PartialEq>(
    focus: &mut F,
    event: &Event,
) {
    focus.update_from_event(event);
}

/// `clutter_focus_notify_grab`. The C version takes a `ClutterGrab *`
/// and extracts its actor; here the caller passes the grab's actor
/// directly (and the previous grab's actor), keeping this module free
/// of the `Grab` type.
pub fn notify_grab<F: Focus<Id> + ?Sized, Id: Copy + Eq + PartialEq>(
    focus: &mut F,
    grab_actor: Option<Id>,
    old_grab_actor: Option<Id>,
) {
    focus.notify_grab(grab_actor, old_grab_actor);
}

#[cfg(test)]
mod tests {
    use super::super::event::{EventFlags, KeyEvent, ModifierSet, ModifierType};
    use super::*;

    /// A minimal key-focus policy: tracks the focused actor, returns
    /// true only when the focus actually moves.
    struct KeyFocus {
        current: Option<u32>,
    }

    impl Focus<u32> for KeyFocus {
        fn set_current_actor(
            &mut self,
            actor: Option<u32>,
            _source_device: Option<DeviceId>,
            _time_ms: u32,
        ) -> bool {
            if self.current == actor {
                return false;
            }
            self.current = actor;
            true
        }
        fn current_actor(&self) -> Option<u32> {
            self.current
        }
    }

    fn key_event() -> Event {
        Event::Key(KeyEvent {
            time_us: 1000,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(1)),
            raw_modifiers: ModifierSet::default(),
            modifier_state: ModifierType::NONE,
            keyval: 65,
            hardware_keycode: 50,
            unicode_value: 0,
            evdev_code: 30,
        })
    }

    #[test]
    fn set_current_actor_returns_true_on_move() {
        let mut f = KeyFocus { current: None };
        assert!(set_current_actor(&mut f, Some(1u32), None, CURRENT_TIME));
        assert_eq!(current_actor(&f), Some(1u32));
        // Same actor -> false.
        assert!(!set_current_actor(&mut f, Some(1u32), None, CURRENT_TIME));
        // Different actor -> true.
        assert!(set_current_actor(&mut f, Some(2u32), None, CURRENT_TIME));
        assert_eq!(current_actor(&f), Some(2u32));
        // Clear -> true.
        assert!(set_current_actor(&mut f, None, None, CURRENT_TIME));
        assert_eq!(current_actor(&f), None);
    }

    #[test]
    fn propagate_event_default_is_noop() {
        // A focus impl that doesn't override propagate_event.
        struct Bare;
        impl Focus<u32> for Bare {
            fn set_current_actor(&mut self, _: Option<u32>, _: Option<DeviceId>, _: u32) -> bool {
                false
            }
            fn current_actor(&self) -> Option<u32> {
                None
            }
        }
        let mut f = Bare;
        // Should not panic (default no-op).
        propagate_event(&mut f, &key_event());
    }

    #[test]
    fn update_from_event_default_is_noop() {
        struct Bare;
        impl Focus<u32> for Bare {
            fn set_current_actor(&mut self, _: Option<u32>, _: Option<DeviceId>, _: u32) -> bool {
                false
            }
            fn current_actor(&self) -> Option<u32> {
                None
            }
        }
        let mut f = Bare;
        update_from_event(&mut f, &key_event());
    }

    #[test]
    fn notify_grab_default_is_noop() {
        struct Bare;
        impl Focus<u32> for Bare {
            fn set_current_actor(&mut self, _: Option<u32>, _: Option<DeviceId>, _: u32) -> bool {
                false
            }
            fn current_actor(&self) -> Option<u32> {
                None
            }
        }
        let mut f = Bare;
        notify_grab(&mut f, Some(1u32), None);
    }
}
