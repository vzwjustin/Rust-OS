//! Port of GNOME mutter's `clutter/clutter-transition*.{c,h}` modules.
//!
//! This submodule groups the four transition-related ports:
//!
//! - [`transition`]: base `Transition` class (timeline + interval + target).
//! - [`property_transition`]: `PropertyTransition` — animates a named
//!   property on an animatable target.
//! - [`keyframe_transition`]: `KeyframeTransition` — multi-keyframe
//!   animation with per-segment easing modes.
//! - [`transition_group`]: `TransitionGroup` — drives multiple transitions
//!   from a single shared timeline.

pub mod keyframe_transition;
pub mod property_transition;
pub mod transition;
pub mod transition_group;
