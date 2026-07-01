//! Port of GNOME mutter's `clutter/clutter-property-transition.{c,h}`.
//!
//! `ClutterPropertyTransition` is a `ClutterTransition` subclass that
//! animates a named property on an animatable target.
//!
//! # What's ported
//!
//! - `PropertyTransition` struct: wraps a `Transition` plus a property
//!   name string.
//! - `set_property_name` / `property_name`.
//! - `ensure_initial` — seeds the interval's initial value.
//! - `advance` — delegates to the inner `Transition::advance`.
//! - Full timeline delegation.
//!
//! # What's skipped
//!
//! - GObject property lookup: the property name is stored as a string.
//! - `GValue` interop: our `Interval` works with `f64` directly.

#![allow(dead_code)]

use alloc::string::String;

use super::super::easing::AnimationMode;
use super::super::interval::Interval;
use super::super::timeline::Direction;
use super::transition::{AnimatableHandle, Transition, TransitionResult};

/// Port of `ClutterPropertyTransition`.
#[derive(Debug, Clone)]
pub struct PropertyTransition {
    transition: Transition,
    property_name: String,
}

impl PropertyTransition {
    pub fn new(duration_ms: u32, property_name: &str) -> Self {
        PropertyTransition {
            transition: Transition::new(duration_ms),
            property_name: String::from(property_name),
        }
    }

    pub fn new_with_interval(duration_ms: u32, property_name: &str, interval: Interval) -> Self {
        PropertyTransition {
            transition: Transition::new_with_interval(duration_ms, interval),
            property_name: String::from(property_name),
        }
    }

    pub fn property_name(&self) -> &str { &self.property_name }
    pub fn set_property_name(&mut self, name: &str) { self.property_name = String::from(name); }
    pub fn set_interval(&mut self, interval: Interval) { self.transition.set_interval(interval); }
    pub fn interval(&self) -> Option<Interval> { self.transition.interval() }
    pub fn ensure_initial(&mut self, initial: f64) { self.transition.ensure(initial); }
    pub fn set_animatable(&mut self, handle: AnimatableHandle) { self.transition.set_animatable(handle); }
    pub fn animatable(&self) -> AnimatableHandle { self.transition.animatable() }
    pub fn set_easing_mode(&mut self, mode: AnimationMode) { self.transition.set_easing_mode(mode); }
    pub fn easing_mode(&self) -> AnimationMode { self.transition.easing_mode() }
    pub fn set_remove_on_complete(&mut self, remove: bool) { self.transition.set_remove_on_complete(remove); }
    pub fn remove_on_complete(&self) -> bool { self.transition.remove_on_complete() }
    pub fn start(&mut self) { self.transition.start(); }
    pub fn stop(&mut self) { self.transition.stop(); }
    pub fn pause(&mut self) { self.transition.pause(); }
    pub fn rewind(&mut self) { self.transition.rewind(); }
    pub fn advance(&mut self, delta_ms: u32) -> TransitionResult { self.transition.advance(delta_ms) }
    pub fn duration(&self) -> u32 { self.transition.duration() }
    pub fn set_duration(&mut self, msecs: u32) { self.transition.set_duration(msecs); }
    pub fn is_playing(&self) -> bool { self.transition.is_playing() }
    pub fn direction(&self) -> Direction { self.transition.direction() }
    pub fn set_direction(&mut self, direction: Direction) { self.transition.set_direction(direction); }
    pub fn repeat_count(&self) -> i32 { self.transition.repeat_count() }
    pub fn set_repeat_count(&mut self, count: i32) { self.transition.set_repeat_count(count); }
    pub fn auto_reverse(&self) -> bool { self.transition.auto_reverse() }
    pub fn set_auto_reverse(&mut self, enabled: bool) { self.transition.set_auto_reverse(enabled); }
    pub fn elapsed_time(&self) -> u32 { self.transition.elapsed_time() }
    pub fn inner(&self) -> &Transition { &self.transition }
    pub fn inner_mut(&mut self) -> &mut Transition { &mut self.transition }
    pub fn into_transition(self) -> Transition { self.transition }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_property_name() {
        let pt = PropertyTransition::new(100, "opacity");
        assert_eq!(pt.property_name(), "opacity");
        assert_eq!(pt.duration(), 100);
    }

    #[test]
    fn advance_computes_value() {
        let mut pt = PropertyTransition::new_with_interval(100, "opacity", Interval::new(0.0, 1.0));
        pt.start();
        let r = pt.advance(50);
        let v = r.value.unwrap();
        assert!((v - 0.5).abs() < 1e-10);
    }
}
