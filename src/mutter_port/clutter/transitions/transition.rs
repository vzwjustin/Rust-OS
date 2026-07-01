//! Port of GNOME mutter's `clutter/clutter-transition.{c,h}`.
//!
//! `ClutterTransition` is the base class for timeline-driven value
//! animations. It wraps a `Timeline`, an `Interval` (initial/final values),
//! and an opaque animatable target handle. On each timeline frame it
//! computes the eased progress, interpolates the interval, and returns the
//! result for the caller to apply to the target.
//!
//! # What's ported
//!
//! - Core state: `Timeline` (composition), `Interval`, animatable handle,
//!   `remove_on_complete`, `easing_mode`, `started` flag.
//! - Virtual methods (`compute_value`, `attached`, `detached`) modelled as
//!   `fn` pointers — the direct analog of the C class vtable. The default
//!   `compute_value` does `interval.compute_value(progress)` (linear lerp).
//! - `start` / `stop` / `pause` / `advance` lifecycle: `start` calls
//!   `attached`, `advance` computes the eased value each frame, completion
//!   calls `detached`.
//! - `ensure` — sets the interval's initial value (the caller supplies it
//!   since there is no GObject animatable to read from).
//! - Timeline delegation: `duration`, `set_duration`, `direction`,
//!   `set_direction`, `repeat_count`, `set_repeat_count`, `auto_reverse`,
//!   `set_auto_reverse`, `is_playing`, `rewind`.
//! - `advance_timeline` / `raw_progress` / `eased_progress` exposed for
//!   subclasses (`KeyframeTransition`) that do their own value computation.
//! - `TransitionResult` returned from `advance`: markers, completed flag,
//!   eased progress, and the computed `Option<f64>` value.
//!
//! # What's skipped, with rationale
//!
//! - GObject inheritance: `ClutterTransition` extends `ClutterTimeline` in
//!   C. Here we use composition — `Transition` owns a `Timeline` and
//!   delegates timeline methods — which is the idiomatic Rust approach.
//! - `ClutterAnimatable` target: replaced by an opaque `u64` handle. The
//!   caller receives the computed value via `TransitionResult` and applies
//!   it to whatever target they have. The `attached`/`detached` callbacks
//!   let the caller wire up / tear down the target connection.
//! - GLib signal machinery (`new-frame`, `completed`, `stopped`): replaced
//!   by return values from `advance` and explicit `start`/`stop` calls.
//! - `GValue` / `ClutterInterval` GValue interop: our `Interval` works with
//!   `f64` values directly, so `compute_value` returns `f64`.
//! - `clutter_transition_set_from_value` / `set_to_value`: the caller sets
//!   the interval directly via `set_interval`.

#![allow(dead_code)]

use alloc::vec::Vec;

use super::super::easing::{easing_for_mode, AnimationMode};
use super::super::interval::Interval;
use super::super::timeline::{Direction, Marker, Timeline};

/// Opaque handle to the animatable target (stand-in for
/// `ClutterAnimatable *`). 0 means "no target".
pub type AnimatableHandle = u64;

/// Virtual `compute_value`: given the interval and the eased progress
/// `[0.0, 1.0]`, return the interpolated `f64` value.
///
/// This is the direct analog of `ClutterTransitionClass::compute_value`.
/// Subclasses override it by calling `set_compute_value` with a different
/// function pointer.
pub type ComputeValueFn = fn(&Interval, f64) -> f64;

/// Virtual `attached`: called when the transition is started / attached to
/// an animatable. The implementation can read the target's current state
/// and call `ensure` to seed the interval's initial value.
pub type AttachedFn = fn(&mut Transition);

/// Virtual `detached`: called when the transition stops or completes, after
/// the final value has been computed. The implementation can tear down the
/// target connection.
pub type DetachedFn = fn(&mut Transition);

/// Default `compute_value`: linear interpolation of the interval.
fn default_compute_value(interval: &Interval, progress: f64) -> f64 {
    interval.compute_value(progress)
}

/// Result of `Transition::advance`: the timeline markers crossed, whether the
/// timeline completed, the eased progress, and the computed value (if an
/// interval is set).
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionResult {
    /// Markers crossed during this frame (from the underlying timeline).
    pub markers: Vec<Marker>,
    /// Whether the timeline reached completion this frame.
    pub completed: bool,
    /// Eased progress `[0.0, 1.0]` after applying `easing_mode`.
    pub progress: f64,
    /// The interpolated value, or `None` if no interval is set.
    pub value: Option<f64>,
}

/// Port of `ClutterTransition`: a timeline-driven animation that
/// interpolates an `Interval`'s values and returns the result for the
/// caller to apply to a target.
#[derive(Debug, Clone)]
pub struct Transition {
    /// The underlying timeline that drives this transition.
    timeline: Timeline,
    /// The interval defining initial and final values.
    interval: Option<Interval>,
    /// Opaque handle to the animatable target (0 = none).
    animatable: AnimatableHandle,
    /// Whether to remove the transition when it completes.
    remove_on_complete: bool,
    /// Easing mode applied to the timeline's linear progress.
    easing_mode: AnimationMode,
    /// Virtual: compute the interpolated value at a given progress.
    compute_value_fn: ComputeValueFn,
    /// Virtual: called when the transition is attached to an animatable.
    attached_fn: Option<AttachedFn>,
    /// Virtual: called when the transition is detached from an animatable.
    detached_fn: Option<DetachedFn>,
    /// Whether the transition has been started (and not yet detached).
    started: bool,
}

impl Transition {
    /// Create a new transition with the given duration in milliseconds
    /// and a default linear `AnimationMode`.
    pub fn new(duration_ms: u32) -> Self {
        Transition {
            timeline: Timeline::new(duration_ms),
            interval: None,
            animatable: 0,
            remove_on_complete: false,
            easing_mode: AnimationMode::Linear,
            compute_value_fn: default_compute_value,
            attached_fn: None,
            detached_fn: None,
            started: false,
        }
    }

    /// Create a new transition with a duration and an interval.
    pub fn new_with_interval(duration_ms: u32, interval: Interval) -> Self {
        let mut t = Self::new(duration_ms);
        t.interval = Some(interval);
        t
    }

    /// Set the interval defining initial and final values.
    pub fn set_interval(&mut self, interval: Interval) {
        self.interval = Some(interval);
    }

    /// Get the interval, if set.
    pub fn interval(&self) -> Option<Interval> {
        self.interval
    }

    /// Remove the interval.
    pub fn clear_interval(&mut self) {
        self.interval = None;
    }

    /// Set the opaque animatable target handle.
    pub fn set_animatable(&mut self, handle: AnimatableHandle) {
        self.animatable = handle;
    }

    /// Get the animatable target handle (0 = none).
    pub fn animatable(&self) -> AnimatableHandle {
        self.animatable
    }

    /// Set whether the transition should be removed when it completes.
    pub fn set_remove_on_complete(&mut self, remove: bool) {
        self.remove_on_complete = remove;
    }

    /// Get whether the transition is removed on completion.
    pub fn remove_on_complete(&self) -> bool {
        self.remove_on_complete
    }

    /// Set the easing mode applied to the timeline's linear progress.
    pub fn set_easing_mode(&mut self, mode: AnimationMode) {
        self.easing_mode = mode;
    }

    /// Get the easing mode.
    pub fn easing_mode(&self) -> AnimationMode {
        self.easing_mode
    }

    /// Override the `compute_value` virtual.
    pub fn set_compute_value(&mut self, func: ComputeValueFn) {
        self.compute_value_fn = func;
    }

    /// Set the `attached` callback (called on `start`).
    pub fn set_attached(&mut self, func: AttachedFn) {
        self.attached_fn = Some(func);
    }

    /// Set the `detached` callback (called on `stop` or completion).
    pub fn set_detached(&mut self, func: DetachedFn) {
        self.detached_fn = Some(func);
    }

    /// Port of `clutter_transition_ensure`: set the interval's initial
    /// value.
    pub fn ensure(&mut self, initial: f64) {
        if let Some(ref mut interval) = self.interval {
            interval.set_initial(initial);
        }
    }

    /// Advance the underlying timeline only (no value computation).
    pub fn advance_timeline(&mut self, delta_ms: u32) -> (Vec<Marker>, bool) {
        self.timeline.advance(delta_ms)
    }

    /// Get the raw (linear) progress from the timeline, `[0.0, 1.0]`.
    pub fn raw_progress(&self) -> f64 {
        self.timeline.progress()
    }

    /// Get the eased progress: applies `easing_mode` to the timeline's
    /// linear progress.
    pub fn eased_progress(&self) -> f64 {
        let t = self.timeline.elapsed_time() as f64;
        let d = self.timeline.duration() as f64;
        easing_for_mode(self.easing_mode, t, d)
    }

    /// Called when the timeline completes. Invokes the `detached` callback
    /// if the transition was started.
    pub fn on_completed(&mut self) {
        if self.started {
            if let Some(detached) = self.detached_fn {
                detached(self);
            }
            self.started = false;
        }
    }

    /// Start the transition.
    pub fn start(&mut self) {
        self.timeline.start();
        if !self.started {
            self.started = true;
            if let Some(attached) = self.attached_fn {
                attached(self);
            }
        }
    }

    /// Stop the transition and reset to initial state.
    pub fn stop(&mut self) {
        self.timeline.stop();
        if self.started {
            if let Some(detached) = self.detached_fn {
                detached(self);
            }
            self.started = false;
        }
    }

    /// Pause the transition without resetting state.
    pub fn pause(&mut self) {
        self.timeline.pause();
    }

    /// Rewind to the start (or end if backward direction).
    pub fn rewind(&mut self) {
        self.timeline.rewind();
    }

    /// Advance the transition by `delta_ms` milliseconds.
    pub fn advance(&mut self, delta_ms: u32) -> TransitionResult {
        let (markers, completed) = self.timeline.advance(delta_ms);
        let progress = self.eased_progress();
        let value = self.interval.map(|i| (self.compute_value_fn)(&i, progress));
        if completed {
            self.on_completed();
        }
        TransitionResult { markers, completed, progress, value }
    }

    pub fn duration(&self) -> u32 { self.timeline.duration() }
    pub fn set_duration(&mut self, msecs: u32) { self.timeline.set_duration(msecs); }
    pub fn is_playing(&self) -> bool { self.timeline.is_playing() }
    pub fn direction(&self) -> Direction { self.timeline.direction() }
    pub fn set_direction(&mut self, direction: Direction) { self.timeline.set_direction(direction); }
    pub fn repeat_count(&self) -> i32 { self.timeline.repeat_count() }
    pub fn set_repeat_count(&mut self, count: i32) { self.timeline.set_repeat_count(count); }
    pub fn auto_reverse(&self) -> bool { self.timeline.auto_reverse() }
    pub fn set_auto_reverse(&mut self, enabled: bool) { self.timeline.set_auto_reverse(enabled); }
    pub fn elapsed_time(&self) -> u32 { self.timeline.elapsed_time() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_transition_defaults() {
        let t = Transition::new(100);
        assert_eq!(t.duration(), 100);
        assert!(!t.is_playing());
        assert_eq!(t.easing_mode(), AnimationMode::Linear);
        assert_eq!(t.animatable(), 0);
        assert!(!t.remove_on_complete());
        assert!(t.interval().is_none());
    }

    #[test]
    fn advance_computes_linear_value() {
        let mut t = Transition::new_with_interval(100, Interval::new(0.0, 10.0));
        t.start();
        let r = t.advance(50);
        assert!(!r.completed);
        assert!((r.progress - 0.5).abs() < 1e-10);
        let v = r.value.unwrap();
        assert!((v - 5.0).abs() < 1e-10);
    }

    #[test]
    fn advance_completes_at_duration() {
        let mut t = Transition::new_with_interval(100, Interval::new(0.0, 10.0));
        t.start();
        let r = t.advance(150);
        assert!(r.completed);
        assert!((r.progress - 1.0).abs() < 1e-10);
        let v = r.value.unwrap();
        assert!((v - 10.0).abs() < 1e-10);
    }
}
