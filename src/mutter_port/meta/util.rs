//! Mutter utility functions
//! Ported from meta/util.h
use alloc::{format, string::String, vec::Vec};

/// Utility constants and functions
pub const META_PRIORITY_RESIZE: i32 = -75;
pub const META_PRIORITY_BEFORE_REDRAW: i32 = -60;
pub const META_PRIORITY_REDRAW: i32 = -50;

/// Get current time in milliseconds (monotonic, from kernel boot).
pub fn get_current_time_ms() -> u64 {
    crate::time::uptime_ms()
}

/// Convert X server timestamp to milliseconds
pub fn x_time_to_ms(xtime: u32) -> u64 {
    xtime as u64
}

/// Format time value for logging
pub fn format_time(ms: u64) -> String {
    format!("{}ms", ms)
}

/// Convert milliseconds to seconds.
pub fn ms_to_seconds(ms: u64) -> f64 {
    ms as f64 / 1000.0
}

/// Convert milliseconds to a human-readable string (e.g., "1.5s", "250ms").
pub fn format_duration(ms: u64) -> String {
    if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

/// Clamp a value between min and max.
pub fn clamp_i32(value: i32, min: i32, max: i32) -> i32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Check if a rectangle contains a point.
pub fn rect_contains_point(x: i32, y: i32, rx: i32, ry: i32, rw: i32, rh: i32) -> bool {
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

/// Compute the intersection of two rectangles. Returns None if they
/// don't overlap.
pub fn rect_intersect(
    ax: i32,
    ay: i32,
    aw: i32,
    ah: i32,
    bx: i32,
    by: i32,
    bw: i32,
    bh: i32,
) -> Option<(i32, i32, i32, i32)> {
    let x = ax.max(bx);
    let y = ay.max(by);
    let right = (ax + aw).min(bx + bw);
    let bottom = (ay + ah).min(by + bh);
    if right > x && bottom > y {
        Some((x, y, right - x, bottom - y))
    } else {
        None
    }
}
