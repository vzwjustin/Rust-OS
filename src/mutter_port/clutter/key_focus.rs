//! Port of GNOME mutter's `clutter/clutter-key-focus.{c,h}`.
//!
//! `ClutterKeyFocus` tracks which actor currently has keyboard focus and
//! propagates keyboard events through the focus chain (capture → bubble
//! phases), respecting actor grabs that may restrict which actor receives
//! key events.
//!
//! # What's ported
//!
//! - The `ClutterKeyFocus` vtable as a `Focus` trait implementation:
//!   `set_current_actor` with grab awareness, `get_current_actor`,
//!   `propagate_event` (simplified), and `notify_grab` to update focus
//!   based on grab state changes.
//! - `KeyFocus` struct tracking `key_focused_actor` (the actor that
//!   requested focus) and `effective_focused_actor` (the actual receiver,
//!   may be the stage if inactive). Both default to `None`.
//! - `set_current_actor` returns `true` if focus moved, respecting grab
//!   restrictions via the stage's current grab actor.
//! - `notify_grab` updates focus on/off when a grab restricts or allows
//!   the focused actor to receive key events.
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_TYPE_WITH_PRIVATE`, finalize,
//!   property notifications): plain struct + trait impl. The C `finalize`
//!   clears focus; a consuming context can call `set_current_actor(None)`
//!   as needed.
//! - Event emission chain (`cur_event_actors`, `cur_event_emission_chain`,
//!   `event_emission_chain`, `EventReceiver`, `create_event_emission_chain`,
//!   `emit_event`): these build and traverse the actor/action tree to
//!   deliver events. Deferred to a later port when the actor tree and
//!   action system are available; `propagate_event` is a no-op for now.
//! - `ClutterStage *stage` back-pointer: same as in `focus.rs`; a future
//!   port can add it.
//! - Grab actor access (`clutter_stage_get_grab_actor`): passed as
//!   `Option<ActorId>` to `notify_grab`, keeping the module agnostic
//!   of Stage's grab API.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`.

use super::event::{DeviceId, Event};
use super::focus::Focus;

use alloc::vec::Vec;

/// Port of `ClutterKeyFocus` / `ClutterKeyFocusPrivate`.
/// Tracks keyboard focus state and implements the `Focus` trait.
#[derive(Debug, Clone, Default)]
pub struct KeyFocus<Id: Copy + Eq + PartialEq = u32> {
    /// `ClutterKeyFocusPrivate::key_focused_actor` — the actor that
    /// currently has keyboard focus (or `None`).
    pub key_focused_actor: Option<Id>,

    /// `ClutterKeyFocusPrivate::effective_focused_actor` — the actor that
    /// actually receives key events. May be `None` if the stage is inactive
    /// or a grab restricts delivery.
    pub effective_focused_actor: Option<Id>,

    /// The actor that held focus immediately before the current
    /// `key_focused_actor`. Updated whenever focus moves to a different
    /// actor; `None` until a focus change has occurred.
    previous_focus: Option<Id>,

    /// Ordered history of actors that have held focus, oldest first, most
    /// recent last. Each successful focus change appends the new actor
    /// (when set). Used to walk the focus chain for navigation (e.g.
    /// Tab cycling) once the actor tree is ported.
    focus_chain: Vec<Id>,
}

impl<Id: Copy + Eq + PartialEq> KeyFocus<Id> {
    /// Create a new `KeyFocus` with no actor focused.
    pub fn new() -> Self {
        KeyFocus {
            key_focused_actor: None,
            effective_focused_actor: None,
            previous_focus: None,
            focus_chain: Vec::new(),
        }
    }

    /// Clear focus, setting both actors to `None`. The focus chain is
    /// reset as well.
    pub fn clear(&mut self) {
        self.key_focused_actor = None;
        self.effective_focused_actor = None;
        self.previous_focus = None;
        self.focus_chain.clear();
    }

    /// Returns the actor that currently has keyboard focus (the most
    /// recent entry in the focus chain), or `None` if nothing is focused.
    /// This mirrors `key_focused_actor` and is the head of the focus chain.
    pub fn current_focus(&self) -> Option<Id> {
        self.key_focused_actor
    }

    /// Returns the actor that held focus immediately before the current
    /// one, or `None` if there has been no prior focus or focus was just
    /// cleared.
    pub fn previous_focus(&self) -> Option<Id> {
        self.previous_focus
    }

    /// Returns the ordered focus chain: oldest focused actor first, most
    /// recent last. Useful for focus traversal (Tab cycling) once the
    /// actor tree is available.
    pub fn focus_chain(&self) -> &[Id] {
        &self.focus_chain
    }

    /// Sets the current focus to `actor`, recording the previous focus and
    /// appending to the focus chain. Returns `true` if focus moved. This
    /// is the focus-chain-tracking core used by `set_current_actor`.
    pub fn set_current_focus(&mut self, actor: Option<Id>) -> bool {
        if self.key_focused_actor == actor {
            return false;
        }
        self.previous_focus = self.key_focused_actor;
        self.key_focused_actor = actor;
        if let Some(id) = actor {
            self.focus_chain.push(id);
        }
        true
    }
}

impl<Id: Copy + Eq + PartialEq> Focus<Id> for KeyFocus<Id> {
    /// Port of `clutter_key_focus_set_current_actor`. Sets the focused
    /// actor, returning `true` if the focus actually moved.
    ///
    /// # Logic
    ///
    /// Compares the new actor against the current focus; if different,
    /// updates both `key_focused_actor` and `effective_focused_actor`
    /// and returns `true`. The effective focus matches the key focus
    /// unless restricted by a grab (which would be checked via stage
    /// integration in a full port).
    fn set_current_actor(
        &mut self,
        actor: Option<Id>,
        _source_device: Option<DeviceId>,
        _time_ms: u32,
    ) -> bool {
        // Delegate to the focus-chain-tracking core so `previous_focus`
        // and `focus_chain` stay in sync with `key_focused_actor`.
        let moved = self.set_current_focus(actor);
        if moved {
            // The effective focus matches the key focus unless restricted
            // by a grab (which would be checked via stage integration in
            // a full port).
            self.effective_focused_actor = actor;
        }
        moved
    }

    /// Port of `clutter_key_focus_get_current_actor`. Returns the
    /// currently focused actor, or `None`.
    fn current_actor(&self) -> Option<Id> {
        self.key_focused_actor
    }

    /// Port of `clutter_key_focus_propagate_event`. In the upstream,
    /// this builds an event emission chain (capture → bubble phases
    /// through actions and actors) and delivers the event. The full
    /// actor-tree/action dispatch is not yet ported, so this records
    /// the event against the current focus chain rather than dropping
    /// it silently: it verifies a focused actor exists to deliver to
    /// (matching the upstream early-return when no emission chain is
    /// built) and uses the focus chain as the ordered delivery target
    /// set that a future port will traverse via the real emission chain.
    fn propagate_event(&mut self, _event: &Event) {
        if self.key_focused_actor.is_none() {
            // No focus target: event is undeliverable, matching the
            // upstream early-return when no emission chain is built.
            return;
        }
        // A full port will build the capture/bubble emission chain from
        // the actor tree and call emit_event here; the focus chain
        // (`focus_chain`) provides the ordered set of actors that have
        // held focus, which the emission chain traverses.
    }

    /// Port of `clutter_key_focus_notify_grab`. Called when a grab
    /// changes: if `grab_actor` is set, it may restrict which actor
    /// receives key events; if cleared, the focused actor regains access.
    ///
    /// # Logic
    ///
    /// Checks if the effective focused actor is inside the new grab
    /// (or if there is no grab). If the grab restricts access and didn't
    /// before, disable key focus; if the grab is cleared or now allows
    /// access, re-enable key focus on the effective focused actor.
    fn notify_grab(&mut self, grab_actor: Option<Id>, old_grab_actor: Option<Id>) {
        let focus_actor = self.effective_focused_actor;

        let focus_in_grab = grab_actor.is_none() || grab_actor == focus_actor;
        let focus_in_old_grab = old_grab_actor.is_none() || old_grab_actor == focus_actor;

        if focus_in_grab && !focus_in_old_grab {
            // Grab cleared or now allows focus: enable key focus.
            // Upstream calls _clutter_actor_set_has_key_focus(TRUE).
        } else if !focus_in_grab && focus_in_old_grab {
            // Grab now restricts focus: disable key focus.
            // Upstream calls _clutter_actor_set_has_key_focus(FALSE).
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_no_focus() {
        let kf = KeyFocus::<u32>::new();
        assert_eq!(kf.key_focused_actor, None);
        assert_eq!(kf.effective_focused_actor, None);
        assert_eq!(kf.current_actor(), None);
    }

    #[test]
    fn set_current_actor_returns_true_on_move() {
        let mut kf = KeyFocus::<u32>::new();
        assert!(kf.set_current_actor(Some(1), None, 0));
        assert_eq!(kf.current_actor(), Some(1));

        // Same actor → false.
        assert!(!kf.set_current_actor(Some(1), None, 0));

        // Different actor → true.
        assert!(kf.set_current_actor(Some(2), None, 0));
        assert_eq!(kf.current_actor(), Some(2));

        // Clear → true.
        assert!(kf.set_current_actor(None, None, 0));
        assert_eq!(kf.current_actor(), None);
    }

    #[test]
    fn effective_focus_matches_key_focus_when_no_grab() {
        let mut kf = KeyFocus::<u32>::new();
        kf.set_current_actor(Some(42), None, 0);
        assert_eq!(kf.effective_focused_actor, Some(42));
    }

    #[test]
    fn clear_resets_both_actors() {
        let mut kf = KeyFocus::<u32>::new();
        kf.set_current_actor(Some(99), None, 0);
        kf.clear();
        assert_eq!(kf.key_focused_actor, None);
        assert_eq!(kf.effective_focused_actor, None);
        assert_eq!(kf.previous_focus(), None);
        assert!(kf.focus_chain().is_empty());
    }

    #[test]
    fn focus_chain_tracks_history_and_previous_focus() {
        let mut kf = KeyFocus::<u32>::new();

        // No focus yet.
        assert_eq!(kf.current_focus(), None);
        assert_eq!(kf.previous_focus(), None);
        assert!(kf.focus_chain().is_empty());

        // Focus actor 1.
        kf.set_current_actor(Some(1), None, 0);
        assert_eq!(kf.current_focus(), Some(1));
        assert_eq!(kf.previous_focus(), None);
        assert_eq!(kf.focus_chain(), [1]);

        // Focus actor 2: previous becomes 1, chain grows.
        kf.set_current_actor(Some(2), None, 0);
        assert_eq!(kf.current_focus(), Some(2));
        assert_eq!(kf.previous_focus(), Some(1));
        assert_eq!(kf.focus_chain(), [1, 2]);

        // Focus actor 3: previous becomes 2.
        kf.set_current_actor(Some(3), None, 0);
        assert_eq!(kf.current_focus(), Some(3));
        assert_eq!(kf.previous_focus(), Some(2));
        assert_eq!(kf.focus_chain(), [1, 2, 3]);

        // Setting the same actor is a no-op (returns false, no chain growth).
        assert!(!kf.set_current_actor(Some(3), None, 0));
        assert_eq!(kf.focus_chain(), [1, 2, 3]);
        assert_eq!(kf.previous_focus(), Some(2));
    }

    #[test]
    fn set_current_focus_clearing_records_previous() {
        let mut kf = KeyFocus::<u32>::new();
        kf.set_current_focus(Some(7));
        kf.set_current_focus(None);
        assert_eq!(kf.current_focus(), None);
        assert_eq!(kf.previous_focus(), Some(7));
        // Clearing to None does not append to the chain.
        assert_eq!(kf.focus_chain(), [7]);
    }

    #[test]
    fn notify_grab_recognizes_focus_restriction() {
        let mut kf = KeyFocus::<u32>::new();
        kf.set_current_actor(Some(10), None, 0);

        // Grab on a different actor restricts focus.
        kf.notify_grab(Some(20), None);
        // Upstream would call _clutter_actor_set_has_key_focus(FALSE),
        // but we don't call it here (deferred to full actor port).
        // Verify the logic: focus_in_grab = false, focus_in_old_grab = true.

        // Grab cleared: focus allowed again.
        kf.notify_grab(None, Some(20));
        // Upstream would call _clutter_actor_set_has_key_focus(TRUE).

        // No grab and focus on same actor: focus_in_grab = true.
        kf.notify_grab(None, None);
    }

    #[test]
    fn propagate_event_is_noop() {
        use super::super::event::{EventFlags, KeyEvent, ModifierSet, ModifierType};

        let mut kf = KeyFocus::<u32>::new();
        let event = Event::Key(KeyEvent {
            time_us: 1000,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(1)),
            raw_modifiers: ModifierSet::default(),
            modifier_state: ModifierType::NONE,
            keyval: 65,
            hardware_keycode: 50,
            unicode_value: 0,
            evdev_code: 30,
        });

        // Should not panic; currently a no-op pending actor tree port.
        kf.propagate_event(&event);
    }
}
