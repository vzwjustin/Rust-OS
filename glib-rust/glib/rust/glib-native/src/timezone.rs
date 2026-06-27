//! Time zone matching `gtimezone.h` / `gtimezone.c`.
//!
//! Basic time zone support with fixed offsets and UTC. Full IANA timezone
//! database parsing is deferred (requires file system access).
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;

/// Time type (`GTimeType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeType {
    Standard,
    Daylight,
    Universal,
}

/// A time zone (`GTimeZone`).
///
/// Supports UTC and fixed-offset zones. IANA identifier parsing requires
/// platform file system access and is deferred.
#[derive(Clone, Debug)]
pub struct TimeZone {
    identifier: String,
    offset_seconds: i32,
    is_dst: bool,
}

impl TimeZone {
    /// Create a UTC time zone (`g_time_zone_new_utc`).
    pub fn utc() -> Self {
        Self {
            identifier: "UTC".to_owned(),
            offset_seconds: 0,
            is_dst: false,
        }
    }

    /// Create a time zone from a fixed offset (`g_time_zone_new_offset`).
    pub fn new_offset(seconds: i32) -> Self {
        let sign = if seconds >= 0 { '+' } else { '-' };
        let abs = seconds.unsigned_abs();
        let hours = abs / 3600;
        let minutes = (abs % 3600) / 60;
        let identifier = format!("{}{:02}:{:02}", sign, hours, minutes);
        Self {
            identifier,
            offset_seconds: seconds,
            is_dst: false,
        }
    }

    /// Create a time zone from an identifier (`g_time_zone_new_identifier`).
    ///
    /// Supports "UTC", fixed offsets like "+05:30" or "-08:00", and
    /// returns UTC for unrecognized identifiers (full IANA DB deferred).
    pub fn new_identifier(identifier: &str) -> Self {
        if identifier == "UTC" || identifier == "Z" {
            return Self::utc();
        }

        // Try parsing as a fixed offset: [+-]HH:MM or [+-]HHMM or [+-]HH
        if let Some(tz) = parse_offset(identifier) {
            return tz;
        }

        // Unknown identifier - default to UTC
        Self::utc()
    }

    /// Get the identifier (`g_time_zone_get_identifier`).
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Get the offset for an interval (`g_time_zone_get_offset`).
    pub fn offset(&self, _interval: i32) -> i32 {
        self.offset_seconds
    }

    /// Get the abbreviation for an interval (`g_time_zone_get_abbreviation`).
    pub fn abbreviation(&self, _interval: i32) -> &str {
        &self.identifier
    }

    /// Check if an interval is DST (`g_time_zone_is_dst`).
    pub fn is_dst(&self, _interval: i32) -> bool {
        self.is_dst
    }

    /// Find the interval for a given time (`g_time_zone_find_interval`).
    ///
    /// For fixed-offset zones, there is only one interval (0).
    pub fn find_interval(&self, _type_: TimeType, _time_: i64) -> i32 {
        0
    }

    /// Adjust time for the timezone (`g_time_zone_adjust_time`).
    ///
    /// Returns the adjusted time and sets the interval.
    pub fn adjust_time(&self, type_: TimeType, time_: &mut i64) -> i32 {
        *time_ += self.offset_seconds as i64;
        self.find_interval(type_, *time_)
    }
}

fn parse_offset(s: &str) -> Option<TimeZone> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let sign = match bytes[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let rest = &s[1..];

    // Try HH:MM
    if let Some((h, m)) = rest.split_once(':') {
        let hours: i32 = h.parse().ok()?;
        let minutes: i32 = m.parse().ok()?;
        if hours > 23 || minutes > 59 {
            return None;
        }
        return Some(TimeZone::new_offset(sign * (hours * 3600 + minutes * 60)));
    }

    // Try HHMM (4 digits)
    if rest.len() == 4 {
        let hours: i32 = rest[..2].parse().ok()?;
        let minutes: i32 = rest[2..].parse().ok()?;
        if hours > 23 || minutes > 59 {
            return None;
        }
        return Some(TimeZone::new_offset(sign * (hours * 3600 + minutes * 60)));
    }

    // Try HH (1-2 digits)
    let hours: i32 = rest.parse().ok()?;
    if hours > 23 {
        return None;
    }
    Some(TimeZone::new_offset(sign * hours * 3600))
}

impl PartialEq for TimeZone {
    fn eq(&self, other: &Self) -> bool {
        self.offset_seconds == other.offset_seconds
    }
}

impl Eq for TimeZone {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_zone() {
        let tz = TimeZone::utc();
        assert_eq!(tz.identifier(), "UTC");
        assert_eq!(tz.offset(0), 0);
        assert!(!tz.is_dst(0));
    }

    #[test]
    fn fixed_offset_positive() {
        let tz = TimeZone::new_offset(5 * 3600 + 30 * 60);
        assert_eq!(tz.offset(0), 19800);
        assert_eq!(tz.identifier(), "+05:30");
    }

    #[test]
    fn fixed_offset_negative() {
        let tz = TimeZone::new_offset(-8 * 3600);
        assert_eq!(tz.offset(0), -28800);
        assert_eq!(tz.identifier(), "-08:00");
    }

    #[test]
    fn parse_identifier_utc() {
        let tz = TimeZone::new_identifier("UTC");
        assert_eq!(tz.offset(0), 0);
    }

    #[test]
    fn parse_identifier_offset() {
        let tz = TimeZone::new_identifier("+05:30");
        assert_eq!(tz.offset(0), 19800);
    }

    #[test]
    fn parse_identifier_negative() {
        let tz = TimeZone::new_identifier("-08:00");
        assert_eq!(tz.offset(0), -28800);
    }

    #[test]
    fn parse_identifier_hhmm() {
        let tz = TimeZone::new_identifier("+0530");
        assert_eq!(tz.offset(0), 19800);
    }

    #[test]
    fn parse_identifier_hh() {
        let tz = TimeZone::new_identifier("+05");
        assert_eq!(tz.offset(0), 18000);
    }

    #[test]
    fn parse_identifier_unknown() {
        let tz = TimeZone::new_identifier("America/New_York");
        // Falls back to UTC
        assert_eq!(tz.offset(0), 0);
    }

    #[test]
    fn find_interval() {
        let tz = TimeZone::new_offset(3600);
        assert_eq!(tz.find_interval(TimeType::Universal, 0), 0);
    }

    #[test]
    fn adjust_time() {
        let tz = TimeZone::new_offset(3600);
        let mut t = 0i64;
        let interval = tz.adjust_time(TimeType::Universal, &mut t);
        assert_eq!(t, 3600);
        assert_eq!(interval, 0);
    }
}
