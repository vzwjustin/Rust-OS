//! Port of GNOME mutter's `clutter/clutter-easing.{c,h}`.
//!
//! All 30+ Penner easing functions (`easeInQuad`/`easeOutCubic`/...),
//! the step functions, the cubic-bezier evaluator, the
//! `ClutterAnimationMode` enum, and the mode→function dispatch table.
//!
//! # What's ported
//!
//! - `ClutterAnimationMode` (clutter-enums.h) with values matching the C
//!   sequential numbering (0..`AnimationLast`).
//! - All 30 easing functions from `clutter-easing.c`:
//!   `linear`, `ease_in_quad`/`_out_quad`/`_in_out_quad`, the cubic/
//!   quart/quint/sine/expo/circ/elastic/back/bounce families (in/out/
//!   in-out each), `steps_start`/`steps_end`, and `cubic_bezier`.
//! - The cubic-bezier solver: `x_for_t`/`y_for_t` (the Bernstein
//!   polynomial evaluation) and `t_for_x` (the 30-iteration bisection
//!   from the C `t_for_x`).
//! - The `_clutter_animation_modes` dispatch table as a match in
//!   `easing_for_mode`, plus `easing_name_for_mode`.
//! - The `EASE`/`EASE_IN`/`EASE_OUT`/`EASE_IN_OUT` modes, which the C
//!   table maps to `cubic_bezier` with specific control points; here
//!   they're handled by looking up the standard CSS control points
//!   (documented inline) and calling `cubic_bezier`.
//!
//! # What's skipped, with rationale
//!
//! - `G_PI`/`G_PI_2` constants: replaced with `core::f64::consts::PI`/
//!   `FRAC_PI_2`.
//! - `cos`/`sin`/`sqrt`/`pow` from libm: replaced with the shared
//!   `mutter_port::math` implementations (Taylor-series `sin`/`cos`,
//!   Newton-Raphson `sqrt`, `exp`/`ln`-based `powf`).
//! - `floor` in `ease_steps_end`: replaced with `f64::trunc`-based
//!   `floor_f64` (carried over from `mtk::time_utils`).
//! - The `ClutterEasingFunc` typedef / function-pointer table: a `match`
//!   on `AnimationMode` is the idiomatic Rust equivalent and lets the
//!   compiler inline.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use core::f64::consts::{FRAC_PI_2, PI};

use super::super::math::{cos, floor, powf, sin, sqrt};

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

/// `floor` for `f64` re-exported from `math` for the step functions.
// (The `floor` import above covers this; no local `floor_f64` needed.)

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

/// `clutter_ease_steps_end`: step function with steps aligned to the end
/// of each interval.
pub fn ease_steps_end(t: f64, d: f64, n_steps: i32) -> f64 {
    ease_steps_end_impl(t / d, n_steps)
}

fn ease_steps_end_impl(p: f64, n_steps: i32) -> f64 {
    floor(p * n_steps as f64) / n_steps as f64
}

/// `clutter_ease_steps_start`: step function with steps aligned to the
/// start of each interval.
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

/// `clutter_ease_cubic_bezier`: evaluate the cubic bezier with control
/// points `(x_1, y_1)` and `(x_2, y_2)` (endpoints fixed at `(0,0)` and
/// `(1,1)`) at progress `t/d`.
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

// ---- the standard CSS cubic-bezier control points for EASE/EASE_IN/OUT/IN_OUT ----
// These match the values the C code uses via clutter_timeline_set_ease
// (the EASE/EASE_IN/EASE_OUT/EASE_IN_OUT modes are documented as
// "equivalent to CLUTTER_CUBIC_BEZIER with control points ...").
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

/// `clutter_get_easing_name_for_mode`: the string name for a mode
/// (matching the `_clutter_animation_modes[].name` field). Returns
/// `"custom"` for `CustomMode` and `"sentinel"` for `AnimationLast`.
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

/// `clutter_easing_for_mode`: evaluate the easing function for `mode` at
/// progress `t/d`. The parametrized modes (`Steps`/`StepStart`/`StepEnd`/
/// `CubicBezier`/`Ease`/`EaseIn`/`EaseOut`/`EaseInOut`) use default
/// parameters (1 step for `StepStart`/`StepEnd`, the standard CSS control
/// points for the `Ease*` modes); use the explicit functions
/// (`ease_steps_end`/`cubic_bezier`) for custom parameters.
///
/// `CustomMode` and `AnimationLast` return `t/d` (linear) as a fallback,
/// matching the C null-func guard behavior.
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
        // The C table maps Steps/StepStart/StepEnd to the steps functions
        // with a cast (default 1 step for StepStart/StepEnd).
        AnimationMode::Steps => ease_steps_end(t, d, 1),
        AnimationMode::StepStart => ease_steps_start(t, d, 1),
        AnimationMode::StepEnd => ease_steps_end(t, d, 1),
        AnimationMode::CubicBezier => cubic_bezier(t, d, 0.25, 0.1, 0.25, 1.0),
        AnimationMode::Ease => cubic_bezier(t, d, EASE_X1, EASE_Y1, EASE_X2, EASE_Y2),
        AnimationMode::EaseIn => cubic_bezier(t, d, EASE_IN_X1, EASE_IN_Y1, EASE_IN_X2, EASE_IN_Y2),
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
        // Every easing function should return 0 at t=0 and 1 at t=d.
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
        // ease_in_quad via mode == direct call.
        let d = 100.0;
        assert!(
            (easing_for_mode(AnimationMode::EaseInQuad, 50.0, d) - ease_in_quad(50.0, d)).abs()
                < 1e-10
        );
        // ease_out_bounce via mode.
        assert!(
            (easing_for_mode(AnimationMode::EaseOutBounce, 50.0, d) - ease_out_bounce(50.0, d))
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
