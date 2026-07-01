//! Wall clock — ported from gnome-wall-clock.c
//!
//! Provides formatted time strings from the kernel's RTC-backed system clock.
//! Supports 12h/24h format, weekday, full date, and seconds display options.
//!
//! The upstream uses GSettings for configuration and GDateTime for formatting.
//! We use `crate::time::system_time()` (Unix timestamp from RTC) and format
//! the string directly using calendar arithmetic.

use core::fmt::Write;
use heapless::String as HString;

/// Clock format selection (mirrors GDesktopClockFormat).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockFormat {
    /// 24-hour format (e.g. "14:30")
    TwentyFourHour,
    /// 12-hour format with AM/PM (e.g. "2:30 PM")
    TwelveHour,
}

/// Wall clock configuration — replaces GSettings keys.
#[derive(Debug, Clone, Copy)]
pub struct WallClockConfig {
    pub format: ClockFormat,
    pub show_weekday: bool,
    pub show_full_date: bool,
    pub show_seconds: bool,
    pub time_only: bool,
    pub force_seconds: bool,
}

impl Default for WallClockConfig {
    fn default() -> Self {
        Self {
            format: ClockFormat::TwentyFourHour,
            show_weekday: true,
            show_full_date: true,
            show_seconds: false,
            time_only: false,
            force_seconds: false,
        }
    }
}

/// Wall clock state — tracks the current formatted string and when to refresh.
pub struct WallClock {
    config: WallClockConfig,
    clock_string: HString<64>,
    last_update_second: u64,
}

impl WallClock {
    /// Create a new wall clock with default configuration.
    pub fn new() -> Self {
        let mut clock = Self {
            config: WallClockConfig::default(),
            clock_string: HString::new(),
            last_update_second: 0,
        };
        clock.update();
        clock
    }

    /// Create a wall clock with custom configuration.
    pub fn with_config(config: WallClockConfig) -> Self {
        let mut clock = Self {
            config,
            clock_string: HString::new(),
            last_update_second: 0,
        };
        clock.update();
        clock
    }

    /// Get the current formatted clock string.
    pub fn get_clock(&self) -> &str {
        self.clock_string.as_str()
    }

    /// Get the current configuration.
    pub fn config(&self) -> &WallClockConfig {
        &self.config
    }

    /// Update the configuration and refresh the clock string.
    pub fn set_config(&mut self, config: WallClockConfig) {
        self.config = config;
        self.update();
    }

    /// Set time-only mode (no date or weekday).
    pub fn set_time_only(&mut self, time_only: bool) {
        self.config.time_only = time_only;
        self.update();
    }

    /// Force seconds to always be shown.
    pub fn set_force_seconds(&mut self, force: bool) {
        self.config.force_seconds = force;
        self.update();
    }

    /// Check if the clock needs updating based on the current time.
    /// Returns true if the displayed string would change.
    pub fn needs_update(&self) -> bool {
        let now = crate::time::system_time();
        if self.config.show_seconds || self.config.force_seconds {
            now != self.last_update_second
        } else {
            now / 60 != self.last_update_second / 60
        }
    }

    /// Refresh the clock string from the current system time.
    /// Call this periodically (e.g. every second from the WM tick).
    pub fn update(&mut self) {
        let ts = crate::time::system_time();
        self.last_update_second = ts;
        self.clock_string.clear();
        let _ = self.clock_string.write_str(&format_datetime(
            ts,
            self.config.format,
            self.config.show_weekday && !self.config.time_only,
            self.config.show_full_date && !self.config.time_only,
            self.config.show_seconds || self.config.force_seconds,
        ));
    }

    /// Update only if needed (returns true if updated).
    pub fn update_if_needed(&mut self) -> bool {
        if self.needs_update() {
            self.update();
            true
        } else {
            false
        }
    }
}

impl Default for WallClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a Unix timestamp into a clock string, mirroring
/// `gnome_wall_clock_string_for_datetime()`.
pub fn format_datetime(
    timestamp: u64,
    format: ClockFormat,
    show_weekday: bool,
    show_full_date: bool,
    show_seconds: bool,
) -> HString<64> {
    let dt = unix_to_datetime(timestamp);
    let mut out: HString<64> = HString::new();

    if format == ClockFormat::TwentyFourHour {
        if show_full_date {
            if show_weekday {
                // "%a %b %-e_%R:%S" or "%a %b %-e_%R"
                let _ = write!(
                    out,
                    "{} {} {} {:02}:{:02}",
                    weekday_name(dt.wday),
                    month_name(dt.mon),
                    dt.mday,
                    dt.hour,
                    dt.min
                );
                if show_seconds {
                    let _ = write!(out, ":{:02}", dt.sec);
                }
            } else {
                // "%b %-e_%R:%S" or "%b %-e_%R"
                let _ = write!(
                    out,
                    "{} {} {:02}:{:02}",
                    month_name(dt.mon),
                    dt.mday,
                    dt.hour,
                    dt.min
                );
                if show_seconds {
                    let _ = write!(out, ":{:02}", dt.sec);
                }
            }
        } else if show_weekday {
            // "%a %R:%S" or "%a %R"
            let _ = write!(
                out,
                "{} {:02}:{:02}",
                weekday_name(dt.wday),
                dt.hour,
                dt.min
            );
            if show_seconds {
                let _ = write!(out, ":{:02}", dt.sec);
            }
        } else {
            // "%R:%S" or "%R"
            let _ = write!(out, "{:02}:{:02}", dt.hour, dt.min);
            if show_seconds {
                let _ = write!(out, ":{:02}", dt.sec);
            }
        }
    } else {
        // 12-hour format
        let hour12 = if dt.hour == 0 {
            12
        } else if dt.hour > 12 {
            dt.hour - 12
        } else {
            dt.hour
        };
        let ampm = if dt.hour < 12 { "AM" } else { "PM" };

        if show_full_date {
            if show_weekday {
                // "%a %b %-e_%l:%M:%S %p" or "%a %b %-e_%l:%M %p"
                let _ = write!(
                    out,
                    "{} {} {} {}:{:02}",
                    weekday_name(dt.wday),
                    month_name(dt.mon),
                    dt.mday,
                    hour12,
                    dt.min
                );
                if show_seconds {
                    let _ = write!(out, ":{:02}", dt.sec);
                }
                let _ = write!(out, " {}", ampm);
            } else {
                // "%b %-e_%l:%M:%S %p" or "%b %-e_%l:%M %p"
                let _ = write!(
                    out,
                    "{} {} {}:{:02}",
                    month_name(dt.mon),
                    dt.mday,
                    hour12,
                    dt.min
                );
                if show_seconds {
                    let _ = write!(out, ":{:02}", dt.sec);
                }
                let _ = write!(out, " {}", ampm);
            }
        } else if show_weekday {
            // "%a %l:%M:%S %p" or "%a %l:%M %p"
            let _ = write!(out, "{} {}:{:02}", weekday_name(dt.wday), hour12, dt.min);
            if show_seconds {
                let _ = write!(out, ":{:02}", dt.sec);
            }
            let _ = write!(out, " {}", ampm);
        } else {
            // "%l:%M:%S %p" or "%l:%M %p"
            let _ = write!(out, "{}:{:02}", hour12, dt.min);
            if show_seconds {
                let _ = write!(out, ":{:02}", dt.sec);
            }
            let _ = write!(out, " {}", ampm);
        }
    }

    out
}

/// Broken-down date/time components.
#[derive(Debug, Clone, Copy)]
pub struct DateTimeParts {
    pub year: u32,
    pub mon: u8,   // 1-12
    pub mday: u8,  // 1-31
    pub hour: u8,  // 0-23
    pub min: u8,   // 0-59
    pub sec: u8,   // 0-59
    pub wday: u8,  // 0=Sunday
    pub yday: u16, // 0-365
}

/// Convert a Unix timestamp to broken-down date/time.
/// Uses the civil calendar algorithm from Howard Hinnant's date library.
pub fn unix_to_datetime(ts: u64) -> DateTimeParts {
    let days = (ts / 86400) as i64;
    let secs_of_day = (ts % 86400) as u64;

    let hour = (secs_of_day / 3600) as u8;
    let min = ((secs_of_day % 3600) / 60) as u8;
    let sec = (secs_of_day % 60) as u8;

    // Days since 1970-01-01 → calendar date using the civil_from_days algorithm
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y } as u32;

    let mday = d as u8;
    let mon = m as u8;

    // Day of week: 1970-01-01 was Thursday (4)
    let wday = ((days % 7 + 4) % 7) as u8;
    let wday = if days >= 0 { wday } else { (wday + 7) % 7 };

    // Day of year
    let yday = day_of_year(year, mon, mday);

    DateTimeParts {
        year,
        mon,
        mday,
        hour,
        min,
        sec,
        wday,
        yday,
    }
}

/// Calculate day of year (0-based) for a given date.
fn day_of_year(year: u32, month: u8, day: u8) -> u16 {
    let days_in_month: [u16; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut yday = day as u16 - 1;
    for i in 0..(month as usize - 1) {
        yday += days_in_month[i];
    }
    if month > 2 && is_leap_year(year) {
        yday += 1;
    }
    yday
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Full weekday name (Sunday=0).
fn weekday_name(wday: u8) -> &'static str {
    match wday % 7 {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => "???",
    }
}

/// Abbreviated month name (January=1).
fn month_name(mon: u8) -> &'static str {
    match mon {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_unix_epoch() {
        let dt = unix_to_datetime(0);
        assert_eq!(dt.year, 1970);
        assert_eq!(dt.mon, 1);
        assert_eq!(dt.mday, 1);
        assert_eq!(dt.hour, 0);
        assert_eq!(dt.min, 0);
        assert_eq!(dt.sec, 0);
        assert_eq!(dt.wday, 4); // Thursday
    }

    fn test_known_timestamp() {
        // 2024-01-01 00:00:00 UTC = 1704067200
        let dt = unix_to_datetime(1704067200);
        assert_eq!(dt.year, 2024);
        assert_eq!(dt.mon, 1);
        assert_eq!(dt.mday, 1);
        assert_eq!(dt.wday, 1); // Monday
    }

    fn test_format_24h() {
        let s = format_datetime(1704067200, ClockFormat::TwentyFourHour, false, false, false);
        assert_eq!(s.as_str(), "00:00");
    }

    fn test_format_12h() {
        let s = format_datetime(1704067200, ClockFormat::TwelveHour, false, false, false);
        assert_eq!(s.as_str(), "12:00 AM");
    }

    fn test_format_with_seconds() {
        let s = format_datetime(1704067200, ClockFormat::TwentyFourHour, false, false, true);
        assert_eq!(s.as_str(), "00:00:00");
    }
}
