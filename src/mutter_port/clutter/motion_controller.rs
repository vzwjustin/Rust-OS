//! Port of GNOME mutter's `clutter/clutter-motion-controller.{c,h}`.
//!
//! `ClutterMotionController` is an action that emits `enter`/`motion`/
//! `leave` signals as a sprite (pointer/touch) moves across the actor
//! it's attached to. It's the base building block for hover tracking.
//!
//! # What's ported
//!
//! - The `ClutterMotionControllerClass::handle_event` override as a
//!   `MotionController` implementing the ported `Action` trait. It
//!   matches the C `clutter_motion_controller_handle_event`:
//!   - Filters to `Motion`/`Enter`/`Leave` event types (other events
//!     propagate).
//!   - Extracts the event coords.
//!   - Dispatches to an `on_enter`/`on_motion`/`on_leave` callback set
//!     (the C `g_signal_emit` becomes a callback struct, since there's
//!     no signal system in this port).
//!   - Always returns `false` (propagate), matching the C
//!     `CLUTTER_EVENT_PROPAGATE` return.
//! - The `enter`/`motion`/`leave` "signals" as a `MotionHandler` trait
//!   with three methods. A concrete handler (e.g. a hover-highlight
//!   effect) implements this to receive motion notifications.
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_TYPE`, `g_signal_new`): the signals
//!   become a trait + callbacks.
//! - `ClutterSprite *sprite`: `ClutterSprite` isn't ported (it's a
//!   touch/pointer tracking type); the handler receives a `u32`
//!   sprite-id placeholder, matching the convention used in `action.rs`.
//! - `clutter_actor_transform_stage_point`: the C version transforms
//!   stage coords to actor-relative coords before emitting the signal.
//!   No actor-transform machinery is ported, so the controller passes
//!   the raw stage coords and the handler is responsible for any
//!   transform (a future port can add a transform step here).
//! - `clutter_backend_get_sprite`: the sprite lookup needs the backend;
//!   the controller takes the sprite-id from the event's
//!   `source_device` (as a stand-in for the sprite), since the backend
//!   sprite lookup isn't ported.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use super::action::Action;
use super::actor_meta::ActorMeta;
use super::event::{DeviceId, Event, EventType};

/// The "signals" of `ClutterMotionController`, as a trait. Implement
/// this to receive `enter`/`motion`/`leave` notifications. The `sprite`
/// parameter is a placeholder for `ClutterSprite *` (a `u32` sprite-id).
pub trait MotionHandler {
    /// `ClutterMotionController::enter`: a sprite entered the actor at
    /// actor-relative coords `(x, y)`.
    fn on_enter(&mut self, _sprite: u32, _x: f32, _y: f32) {}
    /// `ClutterMotionController::motion`: a sprite moved within the
    /// actor to `(x, y)`.
    fn on_motion(&mut self, _sprite: u32, _x: f32, _y: f32) {}
    /// `ClutterMotionController::leave`: a sprite left the actor.
    fn on_leave(&mut self, _sprite: u32) {}
}

/// A no-op handler for tests and as a default.
#[derive(Debug, Default)]
pub struct NullMotionHandler;
impl MotionHandler for NullMotionHandler {}

/// Port of `ClutterMotionController`. Implements `Action` so it can be
/// attached to an actor via the action-dispatch machinery.
///
/// The handler is held by reference; a concrete handler (e.g. a
/// hover-highlight effect) is set via `set_handler`.
pub struct MotionController<H: MotionHandler> {
    handler: H,
}

impl<H: MotionHandler> MotionController<H> {
    /// `clutter_motion_controller_new` (minus the GObject plumbing):
    /// construct a motion controller with the given handler.
    pub fn new(handler: H) -> Self {
        MotionController { handler }
    }

    /// Borrow the handler (for inspection).
    pub fn handler(&self) -> &H {
        &self.handler
    }

    /// Mutably borrow the handler (for stateful handlers).
    pub fn handler_mut(&mut self) -> &mut H {
        &mut self.handler
    }
}

impl<H: MotionHandler> Action for MotionController<H> {
    fn handle_event(&mut self, _meta: &mut ActorMeta, event: &Event) -> bool {
        // The C version extracts the sprite from the backend; here we
        // use the event's source_device id as a stand-in sprite-id.
        let sprite = event.source_device().map(|d| d.0).unwrap_or(0);
        match event.type_() {
            EventType::Enter => {
                if let Some((x, y)) = event.coords() {
                    self.handler.on_enter(sprite, x, y);
                }
            }
            EventType::Motion | EventType::TouchUpdate => {
                if let Some((x, y)) = event.coords() {
                    self.handler.on_motion(sprite, x, y);
                }
            }
            EventType::Leave => {
                self.handler.on_leave(sprite);
            }
            _ => {}
        }
        // Always propagate, matching CLUTTER_EVENT_PROPAGATE.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::super::event::{CrossingEvent, DeviceId, EventFlags, ModifierType};
    use super::*;

    /// A handler that records the last notification it received.
    #[derive(Debug, Default)]
    struct Recorder {
        last_enter: Option<(u32, f32, f32)>,
        last_motion: Option<(u32, f32, f32)>,
        last_leave: Option<u32>,
    }
    impl MotionHandler for Recorder {
        fn on_enter(&mut self, sprite: u32, x: f32, y: f32) {
            self.last_enter = Some((sprite, x, y));
        }
        fn on_motion(&mut self, sprite: u32, x: f32, y: f32) {
            self.last_motion = Some((sprite, x, y));
        }
        fn on_leave(&mut self, sprite: u32) {
            self.last_leave = Some(sprite);
        }
    }

    fn enter_event(x: f32, y: f32) -> Event {
        Event::Crossing(CrossingEvent {
            time_us: 0,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(7)),
            x,
            y,
            sequence: None,
            source: None,
            related: Some(ActorId(0)), // related.is_some() -> Enter
        })
    }

    fn leave_event(x: f32, y: f32) -> Event {
        Event::Crossing(CrossingEvent {
            time_us: 0,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(7)),
            x,
            y,
            sequence: None,
            source: None,
            related: None, // related.is_none() -> Leave
        })
    }

    use super::super::actor::ActorId;

    fn motion_event(x: f32, y: f32) -> Event {
        Event::Motion(super::super::event::MotionEvent {
            time_us: 0,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(7)),
            x,
            y,
            modifier_state: ModifierType::NONE,
            tool: None,
            dx: 0.0,
            dy: 0.0,
            dx_unaccel: 0.0,
            dy_unaccel: 0.0,
            dx_constrained: 0.0,
            dy_constrained: 0.0,
        })
    }

    #[test]
    fn enter_dispatches_on_enter() {
        let mut ctrl = MotionController::new(Recorder::default());
        let mut meta = ActorMeta::new();
        meta.set_actor(Some(ActorId(1)));
        let _ = super::super::action::handle_event(&mut ctrl, &mut meta, &enter_event(10.0, 20.0));
        assert_eq!(ctrl.handler().last_enter, Some((7, 10.0, 20.0)));
        assert!(ctrl.handler().last_motion.is_none());
        assert!(ctrl.handler().last_leave.is_none());
    }

    #[test]
    fn motion_dispatches_on_motion() {
        let mut ctrl = MotionController::new(Recorder::default());
        let mut meta = ActorMeta::new();
        meta.set_actor(Some(ActorId(1)));
        let _ = super::super::action::handle_event(&mut ctrl, &mut meta, &motion_event(30.0, 40.0));
        assert_eq!(ctrl.handler().last_motion, Some((7, 30.0, 40.0)));
        assert!(ctrl.handler().last_enter.is_none());
    }

    #[test]
    fn leave_dispatches_on_leave() {
        let mut ctrl = MotionController::new(Recorder::default());
        let mut meta = ActorMeta::new();
        meta.set_actor(Some(ActorId(1)));
        let _ = super::super::action::handle_event(&mut ctrl, &mut meta, &leave_event(0.0, 0.0));
        assert_eq!(ctrl.handler().last_leave, Some(7));
    }

    #[test]
    fn always_propagates() {
        let mut ctrl = MotionController::new(Recorder::default());
        let mut meta = ActorMeta::new();
        meta.set_actor(Some(ActorId(1)));
        // Enter event -> handled but returns false (propagate).
        assert!(!super::super::action::handle_event(
            &mut ctrl,
            &mut meta,
            &enter_event(0.0, 0.0)
        ));
        // Non-motion event -> not handled, returns false.
        let key = Event::Key(super::super::event::KeyEvent {
            time_us: 0,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(7)),
            raw_modifiers: super::super::event::ModifierSet::default(),
            modifier_state: ModifierType::NONE,
            keyval: 0,
            hardware_keycode: 0,
            unicode_value: 0,
            evdev_code: 0,
        });
        assert!(!super::super::action::handle_event(
            &mut ctrl, &mut meta, &key
        ));
    }

    #[test]
    fn no_actor_set_still_propagates() {
        // The action wrapper guards on actor being set; without an
        // actor, handle_event isn't called and the handler isn't
        // notified.
        let mut ctrl = MotionController::new(Recorder::default());
        let mut meta = ActorMeta::new(); // no actor
        assert!(!super::super::action::handle_event(
            &mut ctrl,
            &mut meta,
            &enter_event(0.0, 0.0)
        ));
        assert!(ctrl.handler().last_enter.is_none());
    }
}
