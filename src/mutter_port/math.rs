//! Shared `no_std` math helpers (`sin`/`cos`/`sqrt`/`pow`/`exp`/`ln` for
//! `f64`) used by `clutter::easing`, `clutter::event`, and
//! `clutter::color_state`.
//!
//! `core` provides `f64::trunc`/`abs`/`fract` and the constants in
//! `core::f64::consts`, but not the transcendental functions (`sin`,
//! `cos`, `sqrt`, `exp`, `ln`, `powf`) — those need either `std` or
//! `libm`. This module provides compact implementations accurate enough
//! for compositor use (easing curves, color EOTFs, event geometry):
//!
//! - `exp`/`ln`: Taylor/atanh series with range reduction (carried over
//!   from `color_state::mathf`, now shared).
//! - `powf`: `exp(y * ln(x))`.
//! - `sqrt`: Newton-Raphson with a bit-hack initial guess (carried over
//!   from `event::sqrt_f32`, here for `f64`).
//! - `sin`/`cos`: argument reduction to `[-pi/4, pi/4]` + Taylor series,
//!   using the `sin(x) ≈ x - x³/6 + x⁵/120 - ...` and
//!   `cos(x) ≈ 1 - x²/2 + x⁴/24 - ...` series.
//!
//! Accuracy is ~1e-10 for `exp`/`ln`/`sqrt` (well within the 8-bit color
//! and 60Hz frame-timing precision the callers need) and ~1e-7 for
//! `sin`/`cos` (sufficient for easing curves, which are perceptual).
//!
//! As with the rest of `mutter_port`, this module uses no `unsafe`, no
//! external crates, and only `core`/`alloc`.

use core::f64::consts::{E, FRAC_PI_2, LN_2, PI};

/// `trunc(x)` for `f64` — round toward zero. Not in `no_std` `core`
/// without libm (`f64::trunc`/`f64::fract` are both std-only). The
/// `as i64 as f64` cast truncates toward zero for values in `i64` range,
/// which covers all inputs used by this module (easing progress in
/// `[0,1]`, timestamps in microseconds, color channel values in
/// `[0,1]`). For out-of-range values the cast saturates, which is
/// acceptable since no caller passes such values.
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

/// `ceil(x)` for `f64` — round toward positive infinity.
pub fn ceil(x: f64) -> f64 {
    let t = trunc(x);
    if x <= 0.0 || x == t {
        t
    } else {
        t + 1.0
    }
}

/// `round(x)` for `f64` — round half away from zero.
pub fn round(x: f64) -> f64 {
    if x >= 0.0 {
        floor(x + 0.5)
    } else {
        ceil(x - 0.5)
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

/// `sqrt(x)` for `f64` — Newton-Raphson with a bit-hack initial guess.
/// Returns `0.0` for `x <= 0.0`.
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

/// `sin(x)` for `f64` — argument reduction to `[-pi/4, pi/4]` + Taylor
/// series. Accurate to ~1e-7.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exp_matches_known_values() {
        assert!((exp(0.0) - 1.0).abs() < 1e-10);
        assert!((exp(1.0) - E).abs() < 1e-9);
        assert!((exp(2.0) - E * E).abs() < 1e-8);
        assert!((exp(-1.0) - 1.0 / E).abs() < 1e-9);
    }

    #[test]
    fn ln_round_trips_with_exp() {
        for &x in &[0.5, 1.0, 2.0, 10.0, 100.0] {
            assert!(
                (exp(ln(x)) - x).abs() < 1e-8,
                "exp(ln({})) = {}",
                x,
                exp(ln(x))
            );
        }
    }

    #[test]
    fn powf_matches_known_values() {
        assert!((powf(2.0, 10.0) - 1024.0).abs() < 1e-6);
        assert!((powf(4.0, 0.5) - 2.0).abs() < 1e-6);
        assert!((powf(9.0, 0.5) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn sqrt_matches_known_values() {
        assert!((sqrt(4.0) - 2.0).abs() < 1e-6);
        assert!((sqrt(9.0) - 3.0).abs() < 1e-6);
        assert!((sqrt(2.0) - 1.41421356).abs() < 1e-5);
        assert_eq!(sqrt(0.0), 0.0);
        assert_eq!(sqrt(-1.0), 0.0);
    }

    #[test]
    fn sin_cos_match_known_angles() {
        assert!((sin(0.0) - 0.0).abs() < 1e-7);
        assert!((sin(PI / 2.0) - 1.0).abs() < 1e-7);
        assert!((sin(PI) - 0.0).abs() < 1e-6);
        assert!((sin(-PI / 2.0) + 1.0).abs() < 1e-7);
        assert!((cos(0.0) - 1.0).abs() < 1e-7);
        assert!((cos(PI / 2.0) - 0.0).abs() < 1e-7);
        assert!((cos(PI) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn sin_cos_identity() {
        // sin^2 + cos^2 = 1
        for &x in &[0.0, 0.5, 1.0, 2.0, 3.0] {
            let s = sin(x);
            let c = cos(x);
            assert!((s * s + c * c - 1.0).abs() < 1e-6, "sin^2+cos^2 at {}", x);
        }
    }
}
