//! Relocated remnants of the abandoned `mutter_port` line-by-line C port.
//!
//! The bulk of `src/mutter_port/` (meta/, mtk/, core/, compositor/,
//! backends/native/, wayland/, x11/, and most of clutter/) was deleted as
//! part of committing to the native in-kernel Wayland compositor
//! (`src/wayland/`, see `docs/boot-to-desktop-in-rust.md`). A handful of
//! small, self-contained `clutter` pieces were genuinely load-bearing for
//! the desktop window manager, so they're relocated here verbatim rather
//! than deleted:
//!
//! - [`frame::Frame`] — per-frame dispatch state, used by
//!   `desktop::Desktop.frame` for frame-count tracking.
//! - [`focus::Focus`] — the keyboard-focus vtable trait, implemented by
//!   `desktop::window_manager::WindowFocus`.
//! - [`grab::{Grab, GrabStack, GrabKind}`] — the input grab stack used by
//!   the window manager for move/resize dragging.
//! - [`event::DeviceId`] — the device-id newtype referenced by the
//!   `Focus` trait signature.
//! - [`easing::{easing_for_mode, AnimationMode}`] — the Penner easing
//!   curve evaluator, used by `desktop::bg_crossfade` for the background
//!   crossfade animation.
//!
//! These are moved as-is (same logic, same tests) from
//! `mutter_port::clutter::{frame,focus,grab,event,easing}` and
//! `mutter_port::math`. The one deliberate trim: the original
//! `focus::Focus` trait also had `propagate_event`/`update_from_event`/
//! `notify_grab` virtuals taking a full `clutter::event::Event` enum
//! (~1100 lines of ported event-type plumbing that nothing in the kernel
//! ever calls or overrides — `WindowFocus` only ever calls
//! `set_current_actor`/`current_actor`). Those unused virtuals (and the
//! `Event` enum they depended on) were dropped rather than dragged along;
//! everything actually exercised by `desktop::window_manager` is
//! unchanged.

/// Minimal device-id newtype, relocated from
/// `mutter_port::clutter::event::DeviceId`. Only the type itself is
/// needed here (as a parameter in the [`focus::Focus`] trait); the rest
/// of the original `event` module (the full `Event` enum and its
/// variants) was dropped — see the module-level doc comment.
pub mod event {
    /// Port of `ClutterInputDevice*` identity, reduced to an opaque id
    /// (`clutter-event.h`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct DeviceId(pub u32);
}

/// Shared `no_std` math helpers (`sin`/`cos`/`sqrt`/`powf`) used by
/// [`easing`]. Relocated from `mutter_port::math` (trimmed to just the
/// functions `easing` uses).
mod math {
    //! Compact `no_std` implementations accurate enough for compositor
    //! use (easing curves): `core` provides `f64::trunc`/`abs`/`fract`
    //! but not the transcendental functions (`sin`, `cos`, `sqrt`,
    //! `exp`, `ln`, `powf`) — those need either `std` or `libm`.
    //!
    //! - `exp`/`ln`: Taylor/atanh series with range reduction.
    //! - `powf`: `exp(y * ln(x))`.
    //! - `sqrt`: Newton-Raphson with a bit-hack initial guess.
    //! - `sin`/`cos`: argument reduction to `[-pi/4, pi/4]` + Taylor
    //!   series.
    //!
    //! Accuracy is ~1e-10 for `exp`/`ln`/`sqrt` and ~1e-7 for `sin`/`cos`
    //! (sufficient for easing curves, which are perceptual).

    use core::f64::consts::{E, FRAC_PI_2, LN_2, PI};

    /// `trunc(x)` for `f64` — round toward zero.
    pub fn trunc(x: f64) -> f64 {
        x as i64 as f64
    }

    /// `floor(x)` for `f64` — round toward negative infinity.
    pub fn floor(x: f64) -> f64 {
        let t = trunc(x);
        if x >= 0.0 || x == t {
            t
        } else {
            t - 1.0
        }
    }

    /// `exp(x)` for `f64` — Taylor series with integer-reduced argument.
    pub fn exp(x: f64) -> f64 {
        if x.is_nan() {
            return x;
        }
        if x == 0.0 {
            return 1.0;
        }
        // Range-reduce: exp(x) = exp(n) * exp(r), r in [-0.5, 0.5].
        let n = if x >= 0.0 {
            (x + 0.5) as i64 as f64
        } else {
            (x - 0.5) as i64 as f64
        };
        let r = x - n;
        // exp(r) via Taylor series.
        let mut term = 1.0_f64;
        let mut sum = 1.0_f64;
        for k in 1..20 {
            term *= r / (k as f64);
            sum += term;
        }
        // exp(n) = E^n via exponentiation by squaring.
        let mut n_i = n as i64;
        let mut neg = false;
        if n_i < 0 {
            neg = true;
            n_i = -n_i;
        }
        let mut base = E;
        let mut result = 1.0_f64;
        while n_i > 0 {
            if n_i & 1 == 1 {
                result *= base;
            }
            base *= base;
            n_i >>= 1;
        }
        if neg {
            result = 1.0 / result;
        }
        result * sum
    }

    /// `ln(x)` for `f64` — range reduction + atanh series.
    pub fn ln(x: f64) -> f64 {
        if x <= 0.0 {
            return f64::NAN;
        }
        if x == 1.0 {
            return 0.0;
        }
        let mut m = x;
        let mut e = 0i32;
        while m >= 2.0 {
            m /= 2.0;
            e += 1;
        }
        while m < 1.0 {
            m *= 2.0;
            e -= 1;
        }
        let z = (m - 1.0) / (m + 1.0);
        let z2 = z * z;
        let mut term = z;
        let mut sum = z;
        for k in 1..20 {
            term *= z2;
            sum += term / ((2 * k + 1) as f64);
        }
        2.0 * sum + (e as f64) * LN_2
    }

    /// `powf(x, y)` for `f64` — `exp(y * ln(x))`. Returns `0.0` for
    /// `x == 0 && y != 0`, `1.0` for `x == 0 && y == 0`, `NAN` for
    /// `x < 0` (the callers in `easing` always pass non-negative `x`).
    pub fn powf(x: f64, y: f64) -> f64 {
        if x == 0.0 {
            return if y == 0.0 { 1.0 } else { 0.0 };
        }
        if x < 0.0 {
            return f64::NAN;
        }
        exp(y * ln(x))
    }

    /// `sqrt(x)` for `f64` — Newton-Raphson with a bit-hack initial
    /// guess. Returns `0.0` for `x <= 0.0`.
    pub fn sqrt(x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        // Initial guess via the fast inverse sqrt bit trick (f64 version).
        let mut guess = {
            let i = x.to_bits();
            // For f64 the magic constant is 0x5fe6eb50c7b537a9.
            let i = 0x5fe6eb50c7b537a9 - (i >> 1);
            f64::from_bits(i)
        };
        let x = x;
        // Three Newton-Raphson refinements: g = 0.5 * (g + x/g).
        guess = 0.5 * (guess + x * guess);
        guess = 0.5 * (guess + x * guess);
        guess = 0.5 * (guess + x * guess);
        guess
    }

    /// `sin(x)` for `f64` — argument reduction to `[-pi/4, pi/4]` +
    /// Taylor series. Accurate to ~1e-7.
    pub fn sin(x: f64) -> f64 {
        // Reduce x to [-pi, pi].
        let mut x = x % (2.0 * PI);
        if x > PI {
            x -= 2.0 * PI;
        } else if x < -PI {
            x += 2.0 * PI;
        }
        // Further reduce to [-pi/4, pi/4] using identities.
        if x > FRAC_PI_2 {
            x = PI - x;
        } else if x < -FRAC_PI_2 {
            x = -PI - x;
        }
        // Taylor series: sin(x) = x - x^3/3! + x^5/5! - ...
        let x2 = x * x;
        let mut term = x;
        let mut sum = x;
        for k in 1..10 {
            term *= -x2 / ((2 * k) as f64 * (2 * k + 1) as f64);
            sum += term;
        }
        sum
    }

    /// `cos(x)` for `f64` — `sin(x + pi/2)`.
    pub fn cos(x: f64) -> f64 {
        sin(x + FRAC_PI_2)
    }
}

/// Port of GNOME mutter's `clutter/clutter-frame.{c,h}` and
/// `clutter-frame-private.h`, relocated from `mutter_port::clutter::frame`.
///
/// `Frame` is the per-frame state carried through the frame clock
/// dispatch → present pipeline: the frame counter, the expected
/// presentation time, the frame deadline, and the dispatch result.
pub mod frame {
    /// `ClutterFrameResult` (clutter-frame-clock.h). Values match the C
    /// numbering.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    #[repr(u32)]
    pub enum FrameResult {
        /// `CLUTTER_FRAME_RESULT_PENDING_PRESENTED`: the frame was dispatched
        /// and will be presented.
        PendingPresented = 0,
        /// `CLUTTER_FRAME_RESULT_IDLE`: no frame was needed this dispatch.
        #[default]
        Idle = 1,
        /// `CLUTTER_FRAME_RESULT_IGNORED`: the frame was dispatched but the
        /// result is being ignored.
        Ignored = 2,
    }

    /// Port of `ClutterFrame` / `struct _ClutterFrame`.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Frame {
        pub frame_count: i64,
        /// `has_expected_presentation_time` + `expected_presentation_time_us`.
        /// `None` means "no expected presentation time".
        pub expected_presentation_time_us: Option<i64>,
        /// `is_target_presentation_time`: kept for structural fidelity; no
        /// accessor ported (only the frame-clock internals read it).
        pub is_target_presentation_time: bool,
        /// `has_frame_deadline` + `frame_deadline_us`.
        pub frame_deadline_us: Option<i64>,
        /// `has_result` + `result`. `None` means "no result set yet".
        result: Option<FrameResult>,
    }

    impl Default for Frame {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Frame {
        /// `clutter_frame_new`: construct a fresh frame with no
        /// presentation time, no deadline, and no result.
        pub fn new() -> Self {
            Frame {
                frame_count: 0,
                expected_presentation_time_us: None,
                is_target_presentation_time: false,
                frame_deadline_us: None,
                result: None,
            }
        }

        /// `clutter_frame_get_count`.
        pub fn count(&self) -> i64 {
            self.frame_count
        }

        /// `clutter_frame_get_expected_presentation_time`.
        pub fn expected_presentation_time(&self) -> Option<i64> {
            self.expected_presentation_time_us
        }

        /// `clutter_frame_get_frame_deadline`: returns the deadline if set.
        pub fn frame_deadline(&self) -> Option<i64> {
            self.frame_deadline_us
        }

        /// `clutter_frame_has_result`.
        pub fn has_result(&self) -> bool {
            self.result.is_some()
        }

        /// `clutter_frame_get_result`: returns the result if set,
        /// falling back to `Idle` if unset.
        pub fn result(&self) -> FrameResult {
            self.result.unwrap_or(FrameResult::Idle)
        }

        /// Non-panicking variant of `result()` — returns `None` if no
        /// result has been set.
        pub fn try_result(&self) -> Option<FrameResult> {
            self.result
        }

        /// `clutter_frame_set_result`: set the dispatch result. Panics
        /// if a result was already set.
        pub fn set_result(&mut self, result: FrameResult) {
            if self.result.is_some() {
                panic!("Frame::set_result called twice on the same frame");
            }
            self.result = Some(result);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn new_frame_has_no_results_or_times() {
            let f = Frame::new();
            assert_eq!(f.count(), 0);
            assert!(f.expected_presentation_time().is_none());
            assert!(f.frame_deadline().is_none());
            assert!(!f.has_result());
            assert_eq!(f.try_result(), None);
        }

        #[test]
        fn set_result_round_trips() {
            let mut f = Frame::new();
            f.set_result(FrameResult::PendingPresented);
            assert!(f.has_result());
            assert_eq!(f.result(), FrameResult::PendingPresented);
            assert_eq!(f.try_result(), Some(FrameResult::PendingPresented));
        }

        #[test]
        #[should_panic(expected = "called twice")]
        fn set_result_panics_on_double_set() {
            let mut f = Frame::new();
            f.set_result(FrameResult::Idle);
            f.set_result(FrameResult::Ignored);
        }

        #[test]
        fn result_defaults_to_idle_when_unset() {
            let f = Frame::new();
            assert_eq!(f.result(), FrameResult::Idle);
        }

        #[test]
        fn presentation_time_and_deadline_round_trip() {
            let mut f = Frame::new();
            f.expected_presentation_time_us = Some(1_000_000);
            f.frame_deadline_us = Some(900_000);
            assert_eq!(f.expected_presentation_time(), Some(1_000_000));
            assert_eq!(f.frame_deadline(), Some(900_000));
        }
    }
}

/// Port of GNOME mutter's `clutter/clutter-focus.{c,h}`, relocated from
/// `mutter_port::clutter::focus`.
///
/// `Focus` is the abstract keyboard-focus policy vtable: it tracks the
/// currently focused actor. This relocation keeps only the two virtuals
/// actually used by `desktop::window_manager::WindowFocus`
/// (`set_current_actor`/`current_actor`); the original's
/// `propagate_event`/`update_from_event`/`notify_grab` virtuals (which
/// took a full ported `clutter::event::Event` and were never overridden
/// or called anywhere in the kernel) were dropped — see the module-level
/// doc comment on `clutter_compat`.
pub mod focus {
    use super::event::DeviceId;

    /// `CLUTTER_CURRENT_TIME` — the sentinel "current time" value (0)
    /// used by `set_current_actor` when the caller doesn't have a
    /// specific timestamp.
    pub const CURRENT_TIME: u32 = 0;

    /// Port of `ClutterFocusClass` vtable (trimmed, see module docs).
    ///
    /// Generic over the actor-id type (`Id`) so it can be used with
    /// `desktop::window_manager::WindowId` without coupling.
    pub trait Focus<Id: Copy + Eq + PartialEq> {
        /// `ClutterFocusClass::set_current_actor`: set the focused
        /// actor. Returns `true` if the focus changed. `source_device`
        /// is the device that triggered the focus change (or `None`);
        /// `time_ms` is the event timestamp (or `CURRENT_TIME`).
        fn set_current_actor(
            &mut self,
            actor: Option<Id>,
            source_device: Option<DeviceId>,
            time_ms: u32,
        ) -> bool;

        /// `ClutterFocusClass::get_current_actor`: return the currently
        /// focused actor, or `None`.
        fn current_actor(&self) -> Option<Id>;
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
    pub fn current_actor<F: Focus<Id> + ?Sized, Id: Copy + Eq + PartialEq>(
        focus: &F,
    ) -> Option<Id> {
        focus.current_actor()
    }

    #[cfg(test)]
    mod tests {
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
    }
}

/// Port of GNOME mutter's `clutter/clutter-grab.{c,h}` and
/// `clutter-grab-private.h`, plus the grab-list management from
/// `clutter-stage.c`, relocated from `mutter_port::clutter::grab`.
///
/// `Grab` is the opaque handle representing an input grab redirecting
/// events to a specific actor. Grabs form a stack; the topmost grab
/// receives events.
pub mod grab {
    use alloc::vec::Vec;

    /// The kind of grab, distinguishing different grab policies (move,
    /// resize, menu, ...). The Mutter `ClutterGrab` doesn't have this —
    /// it's just an actor grab — but downstream consumers (the window
    /// manager) need to know what kind of grab is active to dispatch
    /// mouse motion correctly. This is a RustOS extension to the ported
    /// type.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum GrabKind {
        /// A generic actor grab (the Mutter default — events go to the
        /// actor, no special motion handling).
        #[default]
        Actor,
        /// A move grab: mouse motion moves the actor.
        Move,
        /// A resize grab: mouse motion resizes the actor.
        Resize,
    }

    /// Port of `ClutterGrab` / `struct _ClutterGrab`.
    ///
    /// Generic over the actor-id type so it can be used with
    /// `desktop::window_manager::WindowId` (a plain `usize` wrapper)
    /// without coupling. The `Id` type must be
    /// `Copy + Eq + PartialEq + Debug` (the operations on a grab stack
    /// only compare and store ids).
    #[derive(Debug, Clone, PartialEq)]
    pub struct Grab<Id: Copy + Eq + PartialEq + core::fmt::Debug> {
        /// The actor receiving grabbed events (`ClutterGrab::actor`).
        pub actor: Id,
        /// `ClutterGrab::owns_actor`: if true, dismissing the grab
        /// should destroy the actor.
        pub owns_actor: bool,
        /// The kind of grab (move/resize/actor). RustOS extension.
        pub kind: GrabKind,
        /// Whether this grab is currently linked into a `GrabStack`.
        pub linked: bool,
    }

    impl<Id: Copy + Eq + PartialEq + core::fmt::Debug> Grab<Id> {
        /// `clutter_grab_new` (minus the stage back-pointer): construct
        /// a grab for `actor`. The grab is not yet linked into a stack;
        /// call `GrabStack::activate` to link it.
        pub fn new(actor: Id, owns_actor: bool) -> Self {
            Grab {
                actor,
                owns_actor,
                kind: GrabKind::Actor,
                linked: false,
            }
        }

        /// Construct a grab with a specific kind (move/resize/actor).
        /// RustOS extension for the window manager's drag/resize grabs.
        pub fn with_kind(actor: Id, owns_actor: bool, kind: GrabKind) -> Self {
            Grab {
                actor,
                owns_actor,
                kind,
                linked: false,
            }
        }

        /// `clutter_grab_is_revoked`: a grab is revoked when it's been
        /// unlinked from the stack.
        pub fn is_revoked(&self) -> bool {
            !self.linked
        }
    }

    /// The grab stack — stands in for `ClutterStagePrivate::topmost_grab`
    /// and the linked-list management in `clutter-stage.c`. The topmost
    /// grab is the last element of `grabs`.
    #[derive(Debug)]
    pub struct GrabStack<Id: Copy + Eq + PartialEq + core::fmt::Debug> {
        grabs: Vec<Grab<Id>>,
    }

    impl<Id: Copy + Eq + PartialEq + core::fmt::Debug> Default for GrabStack<Id> {
        fn default() -> Self {
            GrabStack { grabs: Vec::new() }
        }
    }

    impl<Id: Copy + Eq + PartialEq + core::fmt::Debug> GrabStack<Id> {
        pub fn new() -> Self {
            Self::default()
        }

        /// Number of active grabs.
        pub fn len(&self) -> usize {
            self.grabs.len()
        }

        /// Whether the stack is empty.
        pub fn is_empty(&self) -> bool {
            self.grabs.is_empty()
        }

        /// The topmost (most recently activated) grab, or `None` if empty.
        pub fn topmost(&self) -> Option<&Grab<Id>> {
            self.grabs.last()
        }

        /// The actor receiving grabbed events, or `None` if no grab is
        /// active. Mirrors `clutter_stage_get_grab_actor`.
        pub fn grab_actor(&self) -> Option<Id> {
            self.grabs.last().map(|g| g.actor)
        }

        /// `clutter_grab_activate`: push `grab` onto the stack as the
        /// new topmost. Returns whether the grab was newly linked (a
        /// no-op if it was already linked).
        pub fn activate(&mut self, mut grab: Grab<Id>) -> bool {
            if grab.linked {
                return false; // already active, no-op (C early-return)
            }
            grab.linked = true;
            self.grabs.push(grab);
            true
        }

        /// `clutter_grab_dismiss` / `clutter_stage_unlink_grab`: remove
        /// the grab at `index` from the stack. Returns a
        /// `DismissOutcome` describing what the caller should do.
        /// Returns `None` if `index` is out of bounds or the grab isn't
        /// linked.
        pub fn dismiss(&mut self, index: usize) -> Option<DismissOutcome<Id>> {
            let len = self.grabs.len();
            if index >= len {
                return None;
            }
            let (owns_actor, actor, linked) = {
                let grab = &self.grabs[index];
                (grab.owns_actor, grab.actor, grab.linked)
            };
            if !linked {
                return None;
            }
            let was_topmost = index == len - 1;
            let actor_to_destroy = if owns_actor { Some(actor) } else { None };
            let removed = self.grabs.remove(index);
            debug_assert!(removed.linked);
            let topmost_changed = was_topmost && !self.grabs.is_empty();
            Some(DismissOutcome {
                actor_to_destroy,
                topmost_changed,
            })
        }

        /// Dismiss the topmost grab. Convenience for
        /// `dismiss(len() - 1)`.
        pub fn dismiss_topmost(&mut self) -> Option<DismissOutcome<Id>> {
            let index = self.grabs.len().checked_sub(1)?;
            self.dismiss(index)
        }

        /// Iterate over the grabs from bottom to top.
        pub fn iter(&self) -> impl Iterator<Item = &Grab<Id>> {
            self.grabs.iter()
        }
    }

    /// The result of `GrabStack::dismiss`: what the caller should do
    /// after a grab is removed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DismissOutcome<Id: Copy + Eq + PartialEq + core::fmt::Debug> {
        /// If the dismissed grab owned its actor, this is the actor to
        /// destroy. `None` otherwise.
        pub actor_to_destroy: Option<Id>,
        /// Whether the topmost grab changed (the caller should notify
        /// the new topmost grab).
        pub topmost_changed: bool,
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn new_grab_is_revoked_until_activated() {
            let g = Grab::new(1u32, false);
            assert!(g.is_revoked());
            assert!(!g.linked);
        }

        #[test]
        fn activate_pushes_and_links() {
            let mut stack = GrabStack::new();
            assert!(stack.is_empty());
            assert!(stack.activate(Grab::new(1u32, false)));
            assert_eq!(stack.len(), 1);
            assert_eq!(stack.grab_actor(), Some(1u32));
            assert!(!stack.topmost().unwrap().is_revoked());
        }

        #[test]
        fn activate_on_already_linked_is_noop() {
            let mut stack = GrabStack::new();
            let g = Grab::new(1u32, false);
            assert!(stack.activate(g.clone()));
            assert!(!stack.activate(g));
            assert_eq!(stack.len(), 1);
        }

        #[test]
        fn dismiss_removes_and_reports_outcome() {
            let mut stack = GrabStack::new();
            stack.activate(Grab::new(1u32, false));
            stack.activate(Grab::new(2u32, false));
            let outcome = stack.dismiss(1).unwrap();
            assert_eq!(outcome.actor_to_destroy, None);
            assert!(outcome.topmost_changed);
            assert_eq!(stack.len(), 1);
            assert_eq!(stack.grab_actor(), Some(1u32));
        }

        #[test]
        fn dismiss_only_grab_leaves_empty() {
            let mut stack = GrabStack::new();
            stack.activate(Grab::new(1u32, false));
            let outcome = stack.dismiss(0).unwrap();
            assert_eq!(outcome.actor_to_destroy, None);
            assert!(!outcome.topmost_changed);
            assert!(stack.is_empty());
        }

        #[test]
        fn dismiss_owned_actor_returns_actor_to_destroy() {
            let mut stack = GrabStack::new();
            stack.activate(Grab::new(5u32, true));
            let outcome = stack.dismiss(0).unwrap();
            assert_eq!(outcome.actor_to_destroy, Some(5u32));
        }

        #[test]
        fn dismiss_non_topmost_does_not_change_topmost() {
            let mut stack = GrabStack::new();
            stack.activate(Grab::new(1u32, false)); // index 0
            stack.activate(Grab::new(2u32, false)); // index 1 (topmost)
            let outcome = stack.dismiss(0).unwrap();
            assert!(!outcome.topmost_changed);
            assert_eq!(stack.grab_actor(), Some(2u32));
        }

        #[test]
        fn dismiss_out_of_bounds_returns_none() {
            let mut stack = GrabStack::new();
            stack.activate(Grab::new(1u32, false));
            assert!(stack.dismiss(5).is_none());
            assert!(stack.dismiss(0).is_some());
            assert!(stack.dismiss(0).is_none()); // now empty
        }

        #[test]
        fn dismiss_topmost_convenience() {
            let mut stack = GrabStack::new();
            stack.activate(Grab::new(1u32, false));
            stack.activate(Grab::new(2u32, false));
            let outcome = stack.dismiss_topmost().unwrap();
            assert!(outcome.topmost_changed);
            assert_eq!(stack.grab_actor(), Some(1u32));
        }

        #[test]
        fn iter_goes_bottom_to_top() {
            let mut stack = GrabStack::new();
            stack.activate(Grab::new(1u32, false));
            stack.activate(Grab::new(2u32, false));
            stack.activate(Grab::new(3u32, false));
            let actors: Vec<u32> = stack.iter().map(|g| g.actor).collect();
            assert_eq!(actors, vec![1u32, 2u32, 3u32]);
        }
    }
}

/// Port of GNOME mutter's `clutter/clutter-easing.{c,h}`, relocated from
/// `mutter_port::clutter::easing`.
///
/// All 30+ Penner easing functions, the step functions, the cubic-bezier
/// evaluator, the `AnimationMode` enum, and the mode→function dispatch
/// table.
pub mod easing {
    use core::f64::consts::{FRAC_PI_2, PI};

    use super::math::{cos, floor, powf, sin, sqrt};

    /// `ClutterAnimationMode` (clutter-enums.h). Values match the C
    /// sequential numbering.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    #[repr(u32)]
    pub enum AnimationMode {
        /// `CLUTTER_CUSTOM_MODE = 0` — custom progress function.
        #[default]
        CustomMode = 0,
        /// `CLUTTER_LINEAR` — linear tweening.
        Linear = 1,
        // quadratic
        EaseInQuad = 2,
        EaseOutQuad = 3,
        EaseInOutQuad = 4,
        // cubic
        EaseInCubic = 5,
        EaseOutCubic = 6,
        EaseInOutCubic = 7,
        // quartic
        EaseInQuart = 8,
        EaseOutQuart = 9,
        EaseInOutQuart = 10,
        // quintic
        EaseInQuint = 11,
        EaseOutQuint = 12,
        EaseInOutQuint = 13,
        // sinusoidal
        EaseInSine = 14,
        EaseOutSine = 15,
        EaseInOutSine = 16,
        // exponential
        EaseInExpo = 17,
        EaseOutExpo = 18,
        EaseInOutExpo = 19,
        // circular
        EaseInCirc = 20,
        EaseOutCirc = 21,
        EaseInOutCirc = 22,
        // elastic
        EaseInElastic = 23,
        EaseOutElastic = 24,
        EaseInOutElastic = 25,
        // overshooting cubic
        EaseInBack = 26,
        EaseOutBack = 27,
        EaseInOutBack = 28,
        // exponentially decaying parabolic
        EaseInBounce = 29,
        EaseOutBounce = 30,
        EaseInOutBounce = 31,
        // step functions
        Steps = 32,
        StepStart = 33,
        StepEnd = 34,
        // cubic bezier
        CubicBezier = 35,
        Ease = 36,
        EaseIn = 37,
        EaseOut = 38,
        EaseInOut = 39,
        /// `CLUTTER_ANIMATION_LAST` — sentinel.
        AnimationLast = 40,
    }

    // ---- the 30+ easing functions, mirroring clutter-easing.c exactly ----

    /// `clutter_linear`.
    pub fn linear(t: f64, d: f64) -> f64 {
        t / d
    }

    pub fn ease_in_quad(t: f64, d: f64) -> f64 {
        let p = t / d;
        p * p
    }

    pub fn ease_out_quad(t: f64, d: f64) -> f64 {
        let p = t / d;
        -1.0 * p * (p - 2.0)
    }

    pub fn ease_in_out_quad(t: f64, d: f64) -> f64 {
        let p = t / (d / 2.0);
        if p < 1.0 {
            return 0.5 * p * p;
        }
        let p = p - 1.0;
        -0.5 * (p * (p - 2.0) - 1.0)
    }

    pub fn ease_in_cubic(t: f64, d: f64) -> f64 {
        let p = t / d;
        p * p * p
    }

    pub fn ease_out_cubic(t: f64, d: f64) -> f64 {
        let p = t / d - 1.0;
        p * p * p + 1.0
    }

    pub fn ease_in_out_cubic(t: f64, d: f64) -> f64 {
        let p = t / (d / 2.0);
        if p < 1.0 {
            return 0.5 * p * p * p;
        }
        let p = p - 2.0;
        0.5 * (p * p * p + 2.0)
    }

    pub fn ease_in_quart(t: f64, d: f64) -> f64 {
        let p = t / d;
        p * p * p * p
    }

    pub fn ease_out_quart(t: f64, d: f64) -> f64 {
        let p = t / d - 1.0;
        -1.0 * (p * p * p * p - 1.0)
    }

    pub fn ease_in_out_quart(t: f64, d: f64) -> f64 {
        let p = t / (d / 2.0);
        if p < 1.0 {
            return 0.5 * p * p * p * p;
        }
        let p = p - 2.0;
        -0.5 * (p * p * p * p - 2.0)
    }

    pub fn ease_in_quint(t: f64, d: f64) -> f64 {
        let p = t / d;
        p * p * p * p * p
    }

    pub fn ease_out_quint(t: f64, d: f64) -> f64 {
        let p = t / d - 1.0;
        p * p * p * p * p + 1.0
    }

    pub fn ease_in_out_quint(t: f64, d: f64) -> f64 {
        let p = t / (d / 2.0);
        if p < 1.0 {
            return 0.5 * p * p * p * p * p;
        }
        let p = p - 2.0;
        0.5 * (p * p * p * p * p + 2.0)
    }

    pub fn ease_in_sine(t: f64, d: f64) -> f64 {
        -1.0 * cos(t / d * FRAC_PI_2) + 1.0
    }

    pub fn ease_out_sine(t: f64, d: f64) -> f64 {
        sin(t / d * FRAC_PI_2)
    }

    pub fn ease_in_out_sine(t: f64, d: f64) -> f64 {
        -0.5 * (cos(PI * t / d) - 1.0)
    }

    pub fn ease_in_expo(t: f64, d: f64) -> f64 {
        if t == 0.0 {
            0.0
        } else {
            powf(2.0, 10.0 * (t / d - 1.0))
        }
    }

    pub fn ease_out_expo(t: f64, d: f64) -> f64 {
        if t == d {
            1.0
        } else {
            -powf(2.0, -10.0 * t / d) + 1.0
        }
    }

    pub fn ease_in_out_expo(t: f64, d: f64) -> f64 {
        if t == 0.0 {
            return 0.0;
        }
        if t == d {
            return 1.0;
        }
        let p = t / (d / 2.0);
        if p < 1.0 {
            return 0.5 * powf(2.0, 10.0 * (p - 1.0));
        }
        let p = p - 1.0;
        0.5 * (-powf(2.0, -10.0 * p) + 2.0)
    }

    pub fn ease_in_circ(t: f64, d: f64) -> f64 {
        let p = t / d;
        -1.0 * (sqrt(1.0 - p * p) - 1.0)
    }

    pub fn ease_out_circ(t: f64, d: f64) -> f64 {
        let p = t / d - 1.0;
        sqrt(1.0 - p * p)
    }

    pub fn ease_in_out_circ(t: f64, d: f64) -> f64 {
        let p = t / (d / 2.0);
        if p < 1.0 {
            return -0.5 * (sqrt(1.0 - p * p) - 1.0);
        }
        let p = p - 2.0;
        0.5 * (sqrt(1.0 - p * p) + 1.0)
    }

    pub fn ease_in_elastic(t: f64, d: f64) -> f64 {
        let p = d * 0.3;
        let s = p / 4.0;
        let q = t / d;
        if q == 1.0 {
            return 1.0;
        }
        let q = q - 1.0;
        -(powf(2.0, 10.0 * q) * sin((q * d - s) * (2.0 * PI) / p))
    }

    pub fn ease_out_elastic(t: f64, d: f64) -> f64 {
        let p = d * 0.3;
        let s = p / 4.0;
        let q = t / d;
        if q == 1.0 {
            return 1.0;
        }
        powf(2.0, -10.0 * q) * sin((q * d - s) * (2.0 * PI) / p) + 1.0
    }

    pub fn ease_in_out_elastic(t: f64, d: f64) -> f64 {
        let p = d * (0.3 * 1.5);
        let s = p / 4.0;
        let q = t / (d / 2.0);
        if q == 2.0 {
            return 1.0;
        }
        if q < 1.0 {
            let q = q - 1.0;
            return -0.5 * (powf(2.0, 10.0 * q) * sin((q * d - s) * (2.0 * PI) / p));
        }
        let q = q - 1.0;
        powf(2.0, -10.0 * q) * sin((q * d - s) * (2.0 * PI) / p) * 0.5 + 1.0
    }

    /// The back-easing constant `1.70158` from the C source.
    const BACK_S: f64 = 1.70158;

    pub fn ease_in_back(t: f64, d: f64) -> f64 {
        let p = t / d;
        p * p * ((BACK_S + 1.0) * p - BACK_S)
    }

    pub fn ease_out_back(t: f64, d: f64) -> f64 {
        let p = t / d - 1.0;
        p * p * ((BACK_S + 1.0) * p + BACK_S) + 1.0
    }

    pub fn ease_in_out_back(t: f64, d: f64) -> f64 {
        let p = t / (d / 2.0);
        let s = BACK_S * 1.525;
        if p < 1.0 {
            return 0.5 * (p * p * ((s + 1.0) * p - s));
        }
        let p = p - 2.0;
        0.5 * (p * p * ((s + 1.0) * p + s) + 2.0)
    }

    fn ease_out_bounce_internal(t: f64, d: f64) -> f64 {
        let p = t / d;
        if p < (1.0 / 2.75) {
            return 7.5625 * p * p;
        } else if p < (2.0 / 2.75) {
            let p = p - (1.5 / 2.75);
            return 7.5625 * p * p + 0.75;
        } else if p < (2.5 / 2.75) {
            let p = p - (2.25 / 2.75);
            return 7.5625 * p * p + 0.9375;
        } else {
            let p = p - (2.625 / 2.75);
            return 7.5625 * p * p + 0.984375;
        }
    }

    fn ease_in_bounce_internal(t: f64, d: f64) -> f64 {
        1.0 - ease_out_bounce_internal(d - t, d)
    }

    pub fn ease_in_bounce(t: f64, d: f64) -> f64 {
        ease_in_bounce_internal(t, d)
    }

    pub fn ease_out_bounce(t: f64, d: f64) -> f64 {
        ease_out_bounce_internal(t, d)
    }

    pub fn ease_in_out_bounce(t: f64, d: f64) -> f64 {
        if t < d / 2.0 {
            ease_in_bounce_internal(t * 2.0, d) * 0.5
        } else {
            ease_out_bounce_internal(t * 2.0 - d, d) * 0.5 + 0.5
        }
    }

    /// `clutter_ease_steps_end`: step function with steps aligned to the
    /// end of each interval.
    pub fn ease_steps_end(t: f64, d: f64, n_steps: i32) -> f64 {
        ease_steps_end_impl(t / d, n_steps)
    }

    fn ease_steps_end_impl(p: f64, n_steps: i32) -> f64 {
        floor(p * n_steps as f64) / n_steps as f64
    }

    /// `clutter_ease_steps_start`: step function with steps aligned to
    /// the start of each interval.
    pub fn ease_steps_start(t: f64, d: f64, n_steps: i32) -> f64 {
        1.0 - ease_steps_end_impl(1.0 - (t / d), n_steps)
    }

    // ---- cubic bezier solver ----

    fn x_for_t(t: f64, x_1: f64, x_2: f64) -> f64 {
        let omt = 1.0 - t;
        3.0 * omt * omt * t * x_1 + 3.0 * omt * t * t * x_2 + t * t * t
    }

    fn y_for_t(t: f64, y_1: f64, y_2: f64) -> f64 {
        let omt = 1.0 - t;
        3.0 * omt * omt * t * y_1 + 3.0 * omt * t * t * y_2 + t * t * t
    }

    fn t_for_x(x: f64, x_1: f64, x_2: f64) -> f64 {
        let mut min_t = 0.0_f64;
        let mut max_t = 1.0_f64;
        for _ in 0..30 {
            let guess_t = (min_t + max_t) / 2.0;
            let guess_x = x_for_t(guess_t, x_1, x_2);
            if x < guess_x {
                max_t = guess_t;
            } else {
                min_t = guess_t;
            }
        }
        (min_t + max_t) / 2.0
    }

    /// `clutter_ease_cubic_bezier`: evaluate the cubic bezier with
    /// control points `(x_1, y_1)` and `(x_2, y_2)` (endpoints fixed at
    /// `(0,0)` and `(1,1)`) at progress `t/d`.
    pub fn cubic_bezier(t: f64, d: f64, x_1: f64, y_1: f64, x_2: f64, y_2: f64) -> f64 {
        let p = t / d;
        if p == 0.0 {
            return 0.0;
        }
        if p == 1.0 {
            return 1.0;
        }
        y_for_t(t_for_x(p, x_1, x_2), y_1, y_2)
    }

    // ---- standard CSS cubic-bezier control points for EASE/EASE_IN/OUT/IN_OUT ----
    const EASE_X1: f64 = 0.25;
    const EASE_Y1: f64 = 0.1;
    const EASE_X2: f64 = 0.25;
    const EASE_Y2: f64 = 1.0;

    const EASE_IN_X1: f64 = 0.42;
    const EASE_IN_Y1: f64 = 0.0;
    const EASE_IN_X2: f64 = 1.0;
    const EASE_IN_Y2: f64 = 1.0;

    const EASE_OUT_X1: f64 = 0.0;
    const EASE_OUT_Y1: f64 = 0.0;
    const EASE_OUT_X2: f64 = 0.58;
    const EASE_OUT_Y2: f64 = 1.0;

    const EASE_IN_OUT_X1: f64 = 0.42;
    const EASE_IN_OUT_Y1: f64 = 0.0;
    const EASE_IN_OUT_X2: f64 = 0.58;
    const EASE_IN_OUT_Y2: f64 = 1.0;

    /// `clutter_get_easing_name_for_mode`: the string name for a mode.
    pub fn easing_name_for_mode(mode: AnimationMode) -> &'static str {
        match mode {
            AnimationMode::CustomMode => "custom",
            AnimationMode::Linear => "linear",
            AnimationMode::EaseInQuad => "easeInQuad",
            AnimationMode::EaseOutQuad => "easeOutQuad",
            AnimationMode::EaseInOutQuad => "easeInOutQuad",
            AnimationMode::EaseInCubic => "easeInCubic",
            AnimationMode::EaseOutCubic => "easeOutCubic",
            AnimationMode::EaseInOutCubic => "easeInOutCubic",
            AnimationMode::EaseInQuart => "easeInQuart",
            AnimationMode::EaseOutQuart => "easeOutQuart",
            AnimationMode::EaseInOutQuart => "easeInOutQuart",
            AnimationMode::EaseInQuint => "easeInQuint",
            AnimationMode::EaseOutQuint => "easeOutQuint",
            AnimationMode::EaseInOutQuint => "easeInOutQuint",
            AnimationMode::EaseInSine => "easeInSine",
            AnimationMode::EaseOutSine => "easeOutSine",
            AnimationMode::EaseInOutSine => "easeInOutSine",
            AnimationMode::EaseInExpo => "easeInExpo",
            AnimationMode::EaseOutExpo => "easeOutExpo",
            AnimationMode::EaseInOutExpo => "easeInOutExpo",
            AnimationMode::EaseInCirc => "easeInCirc",
            AnimationMode::EaseOutCirc => "easeOutCirc",
            AnimationMode::EaseInOutCirc => "easeInOutCirc",
            AnimationMode::EaseInElastic => "easeInElastic",
            AnimationMode::EaseOutElastic => "easeOutElastic",
            AnimationMode::EaseInOutElastic => "easeInOutElastic",
            AnimationMode::EaseInBack => "easeInBack",
            AnimationMode::EaseOutBack => "easeOutBack",
            AnimationMode::EaseInOutBack => "easeInOutBack",
            AnimationMode::EaseInBounce => "easeInBounce",
            AnimationMode::EaseOutBounce => "easeOutBounce",
            AnimationMode::EaseInOutBounce => "easeInOutBounce",
            AnimationMode::Steps => "steps",
            AnimationMode::StepStart => "stepStart",
            AnimationMode::StepEnd => "stepEnd",
            AnimationMode::CubicBezier => "cubicBezier",
            AnimationMode::Ease => "ease",
            AnimationMode::EaseIn => "easeIn",
            AnimationMode::EaseOut => "easeOut",
            AnimationMode::EaseInOut => "easeInOut",
            AnimationMode::AnimationLast => "sentinel",
        }
    }

    /// `clutter_easing_for_mode`: evaluate the easing function for
    /// `mode` at progress `t/d`. The parametrized modes use default
    /// parameters (1 step for `StepStart`/`StepEnd`, the standard CSS
    /// control points for the `Ease*` modes); use the explicit functions
    /// (`ease_steps_end`/`cubic_bezier`) for custom parameters.
    ///
    /// `CustomMode` and `AnimationLast` return `t/d` (linear) as a
    /// fallback, matching the C null-func guard behavior.
    pub fn easing_for_mode(mode: AnimationMode, t: f64, d: f64) -> f64 {
        match mode {
            AnimationMode::CustomMode | AnimationMode::AnimationLast => linear(t, d),
            AnimationMode::Linear => linear(t, d),
            AnimationMode::EaseInQuad => ease_in_quad(t, d),
            AnimationMode::EaseOutQuad => ease_out_quad(t, d),
            AnimationMode::EaseInOutQuad => ease_in_out_quad(t, d),
            AnimationMode::EaseInCubic => ease_in_cubic(t, d),
            AnimationMode::EaseOutCubic => ease_out_cubic(t, d),
            AnimationMode::EaseInOutCubic => ease_in_out_cubic(t, d),
            AnimationMode::EaseInQuart => ease_in_quart(t, d),
            AnimationMode::EaseOutQuart => ease_out_quart(t, d),
            AnimationMode::EaseInOutQuart => ease_in_out_quart(t, d),
            AnimationMode::EaseInQuint => ease_in_quint(t, d),
            AnimationMode::EaseOutQuint => ease_out_quint(t, d),
            AnimationMode::EaseInOutQuint => ease_in_out_quint(t, d),
            AnimationMode::EaseInSine => ease_in_sine(t, d),
            AnimationMode::EaseOutSine => ease_out_sine(t, d),
            AnimationMode::EaseInOutSine => ease_in_out_sine(t, d),
            AnimationMode::EaseInExpo => ease_in_expo(t, d),
            AnimationMode::EaseOutExpo => ease_out_expo(t, d),
            AnimationMode::EaseInOutExpo => ease_in_out_expo(t, d),
            AnimationMode::EaseInCirc => ease_in_circ(t, d),
            AnimationMode::EaseOutCirc => ease_out_circ(t, d),
            AnimationMode::EaseInOutCirc => ease_in_out_circ(t, d),
            AnimationMode::EaseInElastic => ease_in_elastic(t, d),
            AnimationMode::EaseOutElastic => ease_out_elastic(t, d),
            AnimationMode::EaseInOutElastic => ease_in_out_elastic(t, d),
            AnimationMode::EaseInBack => ease_in_back(t, d),
            AnimationMode::EaseOutBack => ease_out_back(t, d),
            AnimationMode::EaseInOutBack => ease_in_out_back(t, d),
            AnimationMode::EaseInBounce => ease_in_bounce(t, d),
            AnimationMode::EaseOutBounce => ease_out_bounce(t, d),
            AnimationMode::EaseInOutBounce => ease_in_out_bounce(t, d),
            // The C table maps Steps/StepStart/StepEnd to the steps
            // functions with a cast (default 1 step for
            // StepStart/StepEnd).
            AnimationMode::Steps => ease_steps_end(t, d, 1),
            AnimationMode::StepStart => ease_steps_start(t, d, 1),
            AnimationMode::StepEnd => ease_steps_end(t, d, 1),
            AnimationMode::CubicBezier => cubic_bezier(t, d, 0.25, 0.1, 0.25, 1.0),
            AnimationMode::Ease => cubic_bezier(t, d, EASE_X1, EASE_Y1, EASE_X2, EASE_Y2),
            AnimationMode::EaseIn => {
                cubic_bezier(t, d, EASE_IN_X1, EASE_IN_Y1, EASE_IN_X2, EASE_IN_Y2)
            }
            AnimationMode::EaseOut => {
                cubic_bezier(t, d, EASE_OUT_X1, EASE_OUT_Y1, EASE_OUT_X2, EASE_OUT_Y2)
            }
            AnimationMode::EaseInOut => cubic_bezier(
                t,
                d,
                EASE_IN_OUT_X1,
                EASE_IN_OUT_Y1,
                EASE_IN_OUT_X2,
                EASE_IN_OUT_Y2,
            ),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn endpoints_are_zero_and_one() {
            let d = 1000.0;
            let modes = [
                AnimationMode::Linear,
                AnimationMode::EaseInQuad,
                AnimationMode::EaseOutQuad,
                AnimationMode::EaseInOutQuad,
                AnimationMode::EaseInCubic,
                AnimationMode::EaseOutCubic,
                AnimationMode::EaseInOutCubic,
                AnimationMode::EaseInQuart,
                AnimationMode::EaseOutQuart,
                AnimationMode::EaseInOutQuart,
                AnimationMode::EaseInQuint,
                AnimationMode::EaseOutQuint,
                AnimationMode::EaseInOutQuint,
                AnimationMode::EaseInSine,
                AnimationMode::EaseOutSine,
                AnimationMode::EaseInOutSine,
                AnimationMode::EaseInExpo,
                AnimationMode::EaseOutExpo,
                AnimationMode::EaseInOutExpo,
                AnimationMode::EaseInCirc,
                AnimationMode::EaseOutCirc,
                AnimationMode::EaseInOutCirc,
                AnimationMode::EaseInBack,
                AnimationMode::EaseOutBack,
                AnimationMode::EaseInOutBack,
                AnimationMode::EaseOutBounce,
                AnimationMode::EaseInOutBounce,
                AnimationMode::Ease,
                AnimationMode::EaseIn,
                AnimationMode::EaseOut,
                AnimationMode::EaseInOut,
            ];
            for &mode in &modes {
                let start = easing_for_mode(mode, 0.0, d);
                let end = easing_for_mode(mode, d, d);
                assert!(start.abs() < 1e-6, "{:?}: start = {}", mode, start);
                assert!((end - 1.0).abs() < 1e-6, "{:?}: end = {}", mode, end);
            }
        }

        #[test]
        fn linear_is_identity() {
            assert!((linear(0.0, 100.0) - 0.0).abs() < 1e-10);
            assert!((linear(50.0, 100.0) - 0.5).abs() < 1e-10);
            assert!((linear(100.0, 100.0) - 1.0).abs() < 1e-10);
        }

        #[test]
        fn ease_in_quad_matches_formula() {
            // p=0.5 -> 0.25
            assert!((ease_in_quad(50.0, 100.0) - 0.25).abs() < 1e-10);
        }

        #[test]
        fn ease_out_cubic_matches_formula() {
            // p = 0.5 - 1 = -0.5; (-0.5)^3 + 1 = -0.125 + 1 = 0.875
            assert!((ease_out_cubic(50.0, 100.0) - 0.875).abs() < 1e-10);
        }

        #[test]
        fn ease_in_out_cubic_first_half() {
            // p = 25/50 = 0.5 < 1 -> 0.5 * 0.125 = 0.0625
            assert!((ease_in_out_cubic(25.0, 100.0) - 0.0625).abs() < 1e-10);
        }

        #[test]
        fn ease_out_bounce_bounces() {
            // At t=d, ease_out_bounce = 1.0.
            assert!((ease_out_bounce(100.0, 100.0) - 1.0).abs() < 1e-10);
            // At t=0, ease_out_bounce = 0.0.
            assert!(ease_out_bounce(0.0, 100.0).abs() < 1e-10);
            // Mid-bounce should be between 0 and 1.
            let mid = ease_out_bounce(50.0, 100.0);
            assert!(mid > 0.0 && mid < 1.0);
        }

        #[test]
        fn steps_end_quantizes() {
            // 4 steps: p=0.3 -> floor(0.3*4)/4 = floor(1.2)/4 = 1/4 = 0.25
            assert!((ease_steps_end(30.0, 100.0, 4) - 0.25).abs() < 1e-10);
            // p=0.6 -> floor(2.4)/4 = 2/4 = 0.5
            assert!((ease_steps_end(60.0, 100.0, 4) - 0.5).abs() < 1e-10);
        }

        #[test]
        fn cubic_bezier_endpoints() {
            assert!((cubic_bezier(0.0, 100.0, 0.25, 0.1, 0.25, 1.0) - 0.0).abs() < 1e-10);
            assert!((cubic_bezier(100.0, 100.0, 0.25, 0.1, 0.25, 1.0) - 1.0).abs() < 1e-10);
            // Mid should be between 0 and 1.
            let mid = cubic_bezier(50.0, 100.0, 0.25, 0.1, 0.25, 1.0);
            assert!(mid > 0.0 && mid < 1.0);
        }

        #[test]
        fn easing_name_for_mode_matches_table() {
            assert_eq!(easing_name_for_mode(AnimationMode::Linear), "linear");
            assert_eq!(
                easing_name_for_mode(AnimationMode::EaseInQuad),
                "easeInQuad"
            );
            assert_eq!(easing_name_for_mode(AnimationMode::CustomMode), "custom");
            assert_eq!(
                easing_name_for_mode(AnimationMode::AnimationLast),
                "sentinel"
            );
            assert_eq!(easing_name_for_mode(AnimationMode::Ease), "ease");
        }

        #[test]
        fn easing_for_mode_dispatches_correctly() {
            let d = 100.0;
            assert!(
                (easing_for_mode(AnimationMode::EaseInQuad, 50.0, d) - ease_in_quad(50.0, d)).abs()
                    < 1e-10
            );
            assert!(
                (easing_for_mode(AnimationMode::EaseOutBounce, 50.0, d)
                    - ease_out_bounce(50.0, d))
                .abs()
                    < 1e-10
            );
        }

        #[test]
        fn animation_mode_values_match_c_numbering() {
            assert_eq!(AnimationMode::CustomMode as u32, 0);
            assert_eq!(AnimationMode::Linear as u32, 1);
            assert_eq!(AnimationMode::EaseInQuad as u32, 2);
            assert_eq!(AnimationMode::Steps as u32, 32);
            assert_eq!(AnimationMode::CubicBezier as u32, 35);
            assert_eq!(AnimationMode::AnimationLast as u32, 40);
        }
    }
}
