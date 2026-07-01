//! Mutter utility functions
//! Ported from meta/util.h
use alloc::{string::String, vec::Vec, format};

/// Utility constants and functions
pub const META_PRIORITY_RESIZE: i32 = -75;
pub const META_PRIORITY_BEFORE_REDRAW: i32 = -60;
pub const META_PRIORITY_REDRAW: i32 = -50;

/// Get current time in milliseconds
pub fn get_current_time_ms() -> u64 {
    // TODO: implement with proper clock
    0
}

/// Convert X server timestamp to milliseconds
pub fn x_time_to_ms(xtime: u32) -> u64 {
    xtime as u64
}

/// Format time value for logging
pub fn format_time(ms: u64) -> String {
    format!("{}ms", ms)
}

// TODO: port remaining utility functions
