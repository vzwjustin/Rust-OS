//! Port of GNOME mutter's `clutter/clutter-animatable.{c,h}`.
//!
//! `ClutterAnimatable` is an interface for objects that can be animated by
//! interpolating property values along a timeline. Implementations define how
//! each property transitions from its initial to final state over a progress
//! range [0.0, 1.0].
//!
//! # What's ported
//!
//! - The `ClutterAnimatableInterface` vtable as an `Animatable` trait with
//!   five virtual methods: `find_property`, `get_initial_state`,
//!   `set_final_state`, `interpolate_value`, and `get_actor`.
//! - Default implementations: `find_property`, `get_initial_state`, and
//!   `set_final_state` are no-ops (matching C behavior when not overridden).
//! - `interpolate_value` is mandatory (no default) since it's the core
//!   animation logic.
//! - `get_actor` returns `Option<ActorId>` (the associated actor, or `None`).
//! - Wrapper functions (`run_find_property`, `run_interpolate_value`, etc.)
//!   that invoke the virtual and handle default fallback behavior.
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_INTERFACE`, parent chaining, `GParamSpec`,
//!   `GValue`): no GObject in this port. `find_property` takes/returns opaque
//!   handles; `get_initial_state` and `set_final_state` accept opaque value
//!   references. These can be replaced with concrete types once those are
//!   ported.
//! - `ClutterInterval`: not ported yet. `interpolate_value` accepts an
//!   opaque `&Interval` handle; the caller manages progress computation.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no `unsafe`,
//! no external crates, and only `core`/`alloc`.

use super::actor::ActorId;

/// Opaque stand-in for `GParamSpec` until property metadata is ported.
pub type ParamSpec = ();

/// Opaque stand-in for `GValue` until variant values are ported.
pub type PropertyValue = ();

/// Opaque stand-in for `ClutterInterval` until interval types are ported.
pub type Interval = ();

/// Port of `ClutterAnimatableInterface` vtable. Implement this per animatable
/// type instead of subclassing the GObject. The `ActorId` (associated actor)
/// and any internal state are held separately.
pub trait Animatable {
    /// `ClutterAnimatableInterface::find_property`: retrieve the property
    /// descriptor (GParamSpec equivalent) for a named property.
    /// Default is a no-op (returns `None`), matching behavior when not overridden.
    fn find_property(&self, _property_name: &str) -> Option<&ParamSpec> {
        None
    }

    /// `ClutterAnimatableInterface::get_initial_state`: retrieve the current
    /// (initial) state of a named property and write it into `value`.
    /// Default is a no-op, matching behavior when not overridden.
    fn get_initial_state(&self, _property_name: &str, _value: &mut PropertyValue) {}

    /// `ClutterAnimatableInterface::set_final_state`: set the final state of
    /// a named property from `value`. Default is a no-op, matching behavior
    /// when not overridden.
    fn set_final_state(&mut self, _property_name: &str, _value: &PropertyValue) {}

    /// `ClutterAnimatableInterface::interpolate_value`: interpolate a named
    /// property between the interval's bounds using `progress` [0.0, 1.0].
    /// Must be implemented by concrete types. Returns `true` if interpolation
    /// succeeded, `false` otherwise.
    fn interpolate_value(
        &self,
        property_name: &str,
        interval: &Interval,
        progress: f64,
        value: &mut PropertyValue,
    ) -> bool;

    /// `ClutterAnimatableInterface::get_actor`: retrieve the associated actor.
    /// Default returns `None`, matching behavior when not overridden.
    fn get_actor(&self) -> Option<ActorId> {
        None
    }
}

/// `clutter_animatable_find_property`: invoke the virtual and return its result.
pub fn run_find_property<'a, A: Animatable + ?Sized>(
    animatable: &'a A,
    property_name: &str,
) -> Option<&'a ParamSpec> {
    animatable.find_property(property_name)
}

/// `clutter_animatable_get_initial_state`: invoke the virtual.
pub fn run_get_initial_state<A: Animatable + ?Sized>(
    animatable: &A,
    property_name: &str,
    value: &mut PropertyValue,
) {
    animatable.get_initial_state(property_name, value);
}

/// `clutter_animatable_set_final_state`: invoke the virtual.
pub fn run_set_final_state<A: Animatable + ?Sized>(
    animatable: &mut A,
    property_name: &str,
    value: &PropertyValue,
) {
    animatable.set_final_state(property_name, value);
}

/// `clutter_animatable_interpolate_value`: invoke the virtual and return its result.
pub fn run_interpolate_value<A: Animatable + ?Sized>(
    animatable: &A,
    property_name: &str,
    interval: &Interval,
    progress: f64,
    value: &mut PropertyValue,
) -> bool {
    animatable.interpolate_value(property_name, interval, progress, value)
}

/// `clutter_animatable_get_actor`: invoke the virtual and return its result.
pub fn run_get_actor<A: Animatable + ?Sized>(animatable: &A) -> Option<ActorId> {
    animatable.get_actor()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock animatable that interpolates by linear lerp.
    struct LinearInterp;
    impl Animatable for LinearInterp {
        fn interpolate_value(
            &self,
            _property_name: &str,
            _interval: &Interval,
            progress: f64,
            _value: &mut PropertyValue,
        ) -> bool {
            // Dummy: just check progress is in range.
            progress >= 0.0 && progress <= 1.0
        }
    }

    #[test]
    fn interpolate_value_accepts_valid_progress() {
        let anim = LinearInterp;
        let mut val = ();
        assert!(run_interpolate_value(&anim, "x", &(), 0.5, &mut val));
    }

    #[test]
    fn get_initial_state_defaults_to_noop() {
        let anim = LinearInterp;
        let mut val = ();
        run_get_initial_state(&anim, "x", &mut val); // Should not panic.
    }

    #[test]
    fn get_actor_defaults_to_none() {
        let anim = LinearInterp;
        assert_eq!(run_get_actor(&anim), None);
    }
}
