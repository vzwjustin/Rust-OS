//! Port of mtk/mtk/mtk-utils.{c,h} and mtk/mtk/mtk-time-utils.{c,h} from
//! GNOME mutter.
//!
//! ## Skipped
//!
//! `mtk_compute_viewport_matrix` (mtk-utils.c/.h) is **not** ported: it
//! builds a `graphene_matrix_t` (a 4x4 float matrix from the `graphene`
//! library) used to feed a GPU shader's viewport transform, calling
//! `graphene_matrix_translate`, `graphene_matrix_scale`, and
//! `mtk_monitor_transform_transform_matrix` (itself unported — see
//! `monitor_transform.rs`). There is no graphene-equivalent matrix type in
//! this kernel, and this functionality is tied to the GPU compositing
//! pipeline rather than being pure logic, so it's out of scope here. If/when
//! a 4x4 matrix type exists in this codebase, this function can be ported by
//! translating the float math directly (it does not depend on glib or X11).
//!
//! ## Ported
//!
//! Everything in mtk-time-utils.c/.h is pure integer/float arithmetic with
//! no glib or X11 dependency (aside from `G_USEC_PER_SEC`, which is just the
//! constant 1_000_000) and is ported in full below: the interval-boundary
//! helpers and the unit-conversion inline functions (ns/us/ms <-> each
//! other, plus seconds).

#![allow(dead_code)]

/// `G_USEC_PER_SEC`
pub const USEC_PER_SEC: i64 = 1_000_000;

/// Rounds `boundary_us + k * interval_us` up to the next boundary at or
/// after `reference_us`, for the smallest non-negative integer `k`.
///
/// Mirrors `mtk_extrapolate_next_interval_boundary`.
pub fn extrapolate_next_interval_boundary(
    boundary_us: i64,
    reference_us: i64,
    interval_us: i64,
) -> i64 {
    let num_intervals = core::cmp::max(
        (reference_us - boundary_us + interval_us - 1) / interval_us,
        0,
    );
    boundary_us + num_intervals * interval_us
}

/// Finds the interval boundary (`boundary_us + k * interval_us`, for
/// integer `k`) nearest to `reference_us`.
///
/// Mirrors `mtk_find_nearest_interval_boundary`, which uses `round()` on a
/// floating point division; reproduced here with `f64` to match behavior
/// exactly (including round-half-away-from-zero semantics of C's `round`).
pub fn find_nearest_interval_boundary(
    boundary_us: i64,
    reference_us: i64,
    interval_us: i64,
) -> i64 {
    let num_intervals =
        round_half_away_from_zero((reference_us - boundary_us) as f64 / interval_us as f64) as i64;
    boundary_us + num_intervals * interval_us
}

/// `round()` from C: rounds half away from zero (unlike Rust's `f64::round`
/// default behavior, which is actually the same — both round half away from
/// zero — but spelled out here for clarity and to avoid relying on std).
fn round_half_away_from_zero(x: f64) -> f64 {
    // No `std`/`libm` floor/ceil available; truncation via `as i64` already
    // rounds toward zero, which combined with the +/-0.5 bias below gives
    // C's round-half-away-from-zero semantics.
    if x >= 0.0 {
        (x + 0.5) as i64 as f64
    } else {
        (x - 0.5) as i64 as f64
    }
}

/// Identity helper kept for parity with the C `ns()` inline (useful as a
/// unit-annotation no-op at call sites).
#[inline]
pub const fn ns(ns: u64) -> u64 {
    ns
}

/// Identity helper kept for parity with the C `us()` inline.
#[inline]
pub const fn us(us: i64) -> i64 {
    us
}

/// Identity helper kept for parity with the C `ms()` inline.
#[inline]
pub const fn ms(ms: i64) -> i64 {
    ms
}

/// Milliseconds to microseconds.
#[inline]
pub const fn ms2us(ms: i64) -> i64 {
    us(ms * 1000)
}

/// Microseconds to nanoseconds.
#[inline]
pub const fn us2ns(us: i64) -> u64 {
    ns((us * 1000) as u64)
}

/// Microseconds to milliseconds (truncating).
#[inline]
pub const fn us2ms(us: i64) -> i64 {
    us / 1000
}

/// Nanoseconds to microseconds (truncating).
#[inline]
pub const fn ns2us(ns: u64) -> i64 {
    us((ns / 1000) as i64)
}

/// Seconds to microseconds.
#[inline]
pub const fn s2us(s: i64) -> i64 {
    s * USEC_PER_SEC
}

/// Microseconds to seconds (truncating).
#[inline]
pub const fn us2s(us: i64) -> i64 {
    us / USEC_PER_SEC
}

/// Seconds to nanoseconds.
#[inline]
pub const fn s2ns(s: i64) -> u64 {
    us2ns(s2us(s))
}

/// Seconds to milliseconds.
#[inline]
pub const fn s2ms(s: i64) -> i64 {
    ms(s * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_conversions_round_trip() {
        assert_eq!(ms2us(5), 5_000);
        assert_eq!(us2ms(5_000), 5);
        assert_eq!(us2ns(5), 5_000);
        assert_eq!(ns2us(5_000), 5);
        assert_eq!(s2us(2), 2_000_000);
        assert_eq!(us2s(2_000_000), 2);
        assert_eq!(s2ns(1), 1_000_000_000);
        assert_eq!(s2ms(1), 1_000);
    }

    #[test]
    fn extrapolate_next_boundary_picks_smallest_future_boundary() {
        // boundary at 0, interval 1000us, reference at 2500 -> next boundary
        // at or after reference is 3000.
        assert_eq!(extrapolate_next_interval_boundary(0, 2500, 1000), 3000);
        // reference before boundary -> stays at boundary.
        assert_eq!(extrapolate_next_interval_boundary(1000, 0, 1000), 1000);
        // reference exactly on a boundary -> stays there.
        assert_eq!(extrapolate_next_interval_boundary(0, 3000, 1000), 3000);
    }

    #[test]
    fn nearest_boundary_rounds_to_closer_side() {
        // boundary 0, interval 1000, reference 1400 -> nearest is 1000.
        assert_eq!(find_nearest_interval_boundary(0, 1400, 1000), 1000);
        // reference 1600 -> nearest is 2000.
        assert_eq!(find_nearest_interval_boundary(0, 1600, 1000), 2000);
        // exact half rounds away from zero (matches C round()).
        assert_eq!(find_nearest_interval_boundary(0, 1500, 1000), 2000);
    }
}
