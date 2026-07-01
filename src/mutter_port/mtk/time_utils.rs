//! Port of GNOME mutter's `mtk/mtk-time-utils.{c,h}`.
//!
//! Time-unit conversion helpers and interval-boundary arithmetic used by
//! mutter's frame clock and event timestamp handling.
//!
//! # What's ported
//!
//! - All the `static inline` unit-conversion helpers from the header:
//!   `ns`/`us`/`ms` (identity passthroughs kept for API parity),
//!   `ms2us`, `us2ns`, `us2ms`, `ns2us`, `s2us`, `us2s`, `s2ns`, `s2ms`.
//!   `G_USEC_PER_SEC` is inlined as `USEC_PER_SEC`.
//! - `mtk_extrapolate_next_interval_boundary`: the ceiling-division
//!   next-boundary projection used by the frame clock to compute the next
//!   presentation time from a base boundary and refresh interval.
//! - `mtk_find_nearest_interval_boundary`: the round-to-nearest boundary
//!   helper. The C version uses `round()` from libm; the port uses a
//!   `round_f64` helper implemented inline (add 0.5 and truncate, with
//!   sign handling) since `f64::round` isn't in `no_std` `core`.
//!
//! # What's skipped, with rationale
//!
//! - Nothing else — the whole file is two functions and a set of inline
//!   conversions, all of which port cleanly.
//!
//! As with the rest of `mutter_port`, this module uses no `unsafe`, no
//! external crates, and only `core`/`alloc`.

/// `G_USEC_PER_SEC` — microseconds per second.
pub const USEC_PER_SEC: i64 = 1_000_000;

/// `ns` — identity passthrough (nanoseconds). Kept for API parity with
/// the C `static inline uint64_t ns (...)`.
#[inline]
pub fn ns(n: u64) -> u64 {
    n
}

/// `us` — identity passthrough (microseconds).
#[inline]
pub fn us(u: i64) -> i64 {
    u
}

/// `ms` — identity passthrough (milliseconds).
#[inline]
pub fn ms(m: i64) -> i64 {
    m
}

/// `ms2us` — milliseconds to microseconds.
#[inline]
pub fn ms2us(m: i64) -> i64 {
    us(m * 1000)
}

/// `us2ns` — microseconds to nanoseconds.
#[inline]
pub fn us2ns(u: i64) -> i64 {
    ns(u as u64 * 1000) as i64
}

/// `us2ms` — microseconds to milliseconds (truncating division, matching C).
#[inline]
pub fn us2ms(u: i64) -> i64 {
    u / 1000
}

/// `ns2us` — nanoseconds to microseconds (truncating division, matching C).
#[inline]
pub fn ns2us(n: i64) -> i64 {
    us(n / 1000)
}

/// `s2us` — seconds to microseconds.
#[inline]
pub fn s2us(s: i64) -> i64 {
    s * USEC_PER_SEC
}

/// `us2s` — microseconds to seconds (truncating division, matching C).
#[inline]
pub fn us2s(u: i64) -> i64 {
    u / USEC_PER_SEC
}

/// `s2ns` — seconds to nanoseconds (via `s2us` then `us2ns`, matching C).
#[inline]
pub fn s2ns(s: i64) -> i64 {
    us2ns(s2us(s))
}

/// `s2ms` — seconds to milliseconds.
#[inline]
pub fn s2ms(s: i64) -> i64 {
    ms(s * 1000)
}

/// `mtk_extrapolate_next_interval_boundary`: given a `boundary_us` anchor,
/// a `reference_us` time, and an `interval_us` period, return the next
/// boundary time at or after `reference_us`.
///
/// Mirrors the C ceiling-division: `num_intervals = max((reference -
/// boundary + interval - 1) / interval, 0)`, then
/// `boundary + num_intervals * interval`.
pub fn extrapolate_next_interval_boundary(
    boundary_us: i64,
    reference_us: i64,
    interval_us: i64,
) -> i64 {
    // Guard against a non-positive interval to avoid division by zero or
    // sign flips; the C code assumes a positive interval from the frame
    // clock.
    if interval_us <= 0 {
        return boundary_us;
    }
    let num_intervals = ((reference_us - boundary_us + interval_us - 1) / interval_us).max(0);
    boundary_us + num_intervals * interval_us
}

/// `mtk_find_nearest_interval_boundary`: given a `boundary_us` anchor, a
/// `reference_us` time, and an `interval_us` period, return the boundary
/// time nearest to `reference_us` (rounding half away from zero).
///
/// The C version uses `round()`; the port uses `round_f64` (below) since
/// `f64::round` isn't in `no_std` `core`.
pub fn find_nearest_interval_boundary(
    boundary_us: i64,
    reference_us: i64,
    interval_us: i64,
) -> i64 {
    if interval_us <= 0 {
        return boundary_us;
    }
    let num_intervals = round_f64((reference_us - boundary_us) as f64 / interval_us as f64) as i64;
    boundary_us + num_intervals * interval_us
}

/// `round` for `f64` — round half away from zero. Not in `no_std` `core`;
/// implemented inline. Used only by `find_nearest_interval_boundary`.
fn round_f64(x: f64) -> f64 {
    if x >= 0.0 {
        floor_f64(x + 0.5)
    } else {
        ceil_f64(x - 0.5)
    }
}

/// `f64::trunc` isn't in `no_std` `core` without libm; `as i64` already
/// truncates toward zero for values in range, which is sufficient for
/// the microsecond-scale timestamps this module deals with.
fn trunc_f64(x: f64) -> f64 {
    x as i64 as f64
}

/// `floor` for `f64` — not in `no_std` `core` without libm. Truncation-based
/// implementation used by `round_f64`.
fn floor_f64(x: f64) -> f64 {
    let t = trunc_f64(x);
    if x >= 0.0 || x == t {
        t
    } else {
        t - 1.0
    }
}

/// `ceil` for `f64` — not in `no_std` `core` without libm.
fn ceil_f64(x: f64) -> f64 {
    let t = trunc_f64(x);
    if x <= 0.0 || x == t {
        t
    } else {
        t + 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_conversions_round_trip() {
        assert_eq!(ms2us(5), 5_000);
        assert_eq!(us2ms(5_000), 5);
        assert_eq!(us2ns(3), 3_000);
        assert_eq!(ns2us(3_000), 3);
        assert_eq!(s2us(2), 2_000_000);
        assert_eq!(us2s(2_000_000), 2);
        assert_eq!(s2ns(1), 1_000_000_000);
        assert_eq!(s2ms(2), 2_000);
    }

    #[test]
    fn extrapolate_next_boundary_at_or_after_reference() {
        // boundary=0, interval=16666 (60Hz-ish), reference=50000.
        // num = ceil(50000/16666) = ceil(3.0001) = 4 -> 4*16666 = 66664.
        let b = extrapolate_next_interval_boundary(0, 50_000, 16_666);
        assert_eq!(b, 66_664);
        // reference exactly on a boundary -> that boundary.
        let b = extrapolate_next_interval_boundary(0, 33_332, 16_666);
        assert_eq!(b, 33_332);
        // reference before boundary -> boundary itself (num clamped to 0).
        let b = extrapolate_next_interval_boundary(100, 50, 16_666);
        assert_eq!(b, 100);
    }

    #[test]
    fn find_nearest_boundary_rounds_half_away() {
        // boundary=0, interval=1000, reference=2400 -> nearest is 2000.
        assert_eq!(find_nearest_interval_boundary(0, 2_400, 1_000), 2_000);
        // reference=2600 -> nearest is 3000.
        assert_eq!(find_nearest_interval_boundary(0, 2_600, 1_000), 3_000);
        // reference=2500 -> rounds half away from zero -> 3000.
        assert_eq!(find_nearest_interval_boundary(0, 2_500, 1_000), 3_000);
        // with a non-zero boundary offset.
        assert_eq!(find_nearest_interval_boundary(500, 3_000, 1_000), 3_500);
    }

    #[test]
    fn non_positive_interval_returns_boundary() {
        assert_eq!(extrapolate_next_interval_boundary(10, 100, 0), 10);
        assert_eq!(find_nearest_interval_boundary(10, 100, 0), 10);
    }

    #[test]
    fn round_f64_matches_expected() {
        assert_eq!(round_f64(2.4), 2.0);
        assert_eq!(round_f64(2.5), 3.0);
        assert_eq!(round_f64(2.6), 3.0);
        assert_eq!(round_f64(-2.4), -2.0);
        assert_eq!(round_f64(-2.5), -3.0);
        assert_eq!(round_f64(-2.6), -3.0);
    }
}
