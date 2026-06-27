//! Time zone matching `gtimezone.h` / `gtimezone.c`.
//!
//! Supports UTC, fixed offsets, a small embedded IANA zone table for common
//! identifiers, and TZif/zoneinfo files via [`TimeZone::from_tzif_bytes`] and
//! [`TimeZone::from_zoneinfo_file`]. Fully `no_std` compatible using `alloc`.

use crate::mappedfile::{mapped_file_new, MappedFileError};
use crate::prelude::*;
use crate::tzif::{TzifData, TzifError};

/// Time type (`GTimeType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeType {
    Standard,
    Daylight,
    Universal,
}

/// DST transition rules for embedded IANA zones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DstRule {
    None,
    /// US: second Sunday in March 02:00 – first Sunday in November 02:00.
    Us,
    /// EU: last Sunday in March 01:00 UTC – last Sunday in October 01:00 UTC.
    Eu,
    /// Australia: first Sunday in October – first Sunday in April.
    Au,
}

#[derive(Debug)]
struct EmbeddedZone {
    names: &'static [&'static str],
    std_offset: i32,
    dst_offset: i32,
    dst_rule: DstRule,
}

/// Common IANA zones with standard/DST offsets (no tzdata file required).
const EMBEDDED_ZONES: &[EmbeddedZone] = &[
    EmbeddedZone {
        names: &["UTC", "Z", "GMT", "Etc/UTC", "Etc/GMT"],
        std_offset: 0,
        dst_offset: 0,
        dst_rule: DstRule::None,
    },
    EmbeddedZone {
        names: &["EST", "US/Eastern", "America/New_York", "America/Detroit"],
        std_offset: -5 * 3600,
        dst_offset: -4 * 3600,
        dst_rule: DstRule::Us,
    },
    EmbeddedZone {
        names: &["CST", "US/Central", "America/Chicago"],
        std_offset: -6 * 3600,
        dst_offset: -5 * 3600,
        dst_rule: DstRule::Us,
    },
    EmbeddedZone {
        names: &["MST", "US/Mountain", "America/Denver"],
        std_offset: -7 * 3600,
        dst_offset: -6 * 3600,
        dst_rule: DstRule::Us,
    },
    EmbeddedZone {
        names: &["PST", "US/Pacific", "America/Los_Angeles"],
        std_offset: -8 * 3600,
        dst_offset: -7 * 3600,
        dst_rule: DstRule::Us,
    },
    EmbeddedZone {
        names: &["America/Anchorage", "US/Alaska"],
        std_offset: -9 * 3600,
        dst_offset: -8 * 3600,
        dst_rule: DstRule::Us,
    },
    EmbeddedZone {
        names: &["Europe/London", "GB", "GB-Eire"],
        std_offset: 0,
        dst_offset: 3600,
        dst_rule: DstRule::Eu,
    },
    EmbeddedZone {
        names: &["Europe/Paris", "CET", "Europe/Berlin", "Europe/Rome"],
        std_offset: 3600,
        dst_offset: 2 * 3600,
        dst_rule: DstRule::Eu,
    },
    EmbeddedZone {
        names: &["Europe/Helsinki", "EET"],
        std_offset: 2 * 3600,
        dst_offset: 3 * 3600,
        dst_rule: DstRule::Eu,
    },
    EmbeddedZone {
        names: &["Asia/Tokyo", "Japan"],
        std_offset: 9 * 3600,
        dst_offset: 9 * 3600,
        dst_rule: DstRule::None,
    },
    EmbeddedZone {
        names: &["Asia/Shanghai", "Asia/Hong_Kong", "Asia/Singapore"],
        std_offset: 8 * 3600,
        dst_offset: 8 * 3600,
        dst_rule: DstRule::None,
    },
    EmbeddedZone {
        names: &["Asia/Kolkata", "Asia/Calcutta", "IST"],
        std_offset: 19800, // +05:30
        dst_offset: 19800,
        dst_rule: DstRule::None,
    },
    EmbeddedZone {
        names: &["Australia/Sydney", "Australia/Melbourne"],
        std_offset: 10 * 3600,
        dst_offset: 11 * 3600,
        dst_rule: DstRule::Au,
    },
    EmbeddedZone {
        names: &["Pacific/Auckland", "NZ"],
        std_offset: 12 * 3600,
        dst_offset: 13 * 3600,
        dst_rule: DstRule::None,
    },
];

fn lookup_embedded(name: &str) -> Option<&'static EmbeddedZone> {
    EMBEDDED_ZONES
        .iter()
        .find(|zone| zone.names.iter().any(|n| *n == name))
}

fn nth_weekday_of_month(year: i32, month: u32, weekday: u32, n: u32) -> u32 {
    // weekday: 0=Sun..6=Sat; first weekday on or after the 1st, then +7*(n-1)
    let mut day = 1u32;
    while day <= 31 {
        let dow = unix_weekday(year, month, day);
        if dow == weekday {
            break;
        }
        day += 1;
    }
    day + 7 * (n - 1)
}

fn last_weekday_of_month(year: i32, month: u32, weekday: u32) -> u32 {
    let days = days_in_month(year, month);
    let mut day = days;
    while day > 0 {
        if unix_weekday(year, month, day) == weekday {
            return day;
        }
        day -= 1;
    }
    1
}

fn days_in_month(year: i32, month: u32) -> u32 {
    const DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if month == 2 && is_leap_year(year) {
        29
    } else {
        DAYS[(month - 1) as usize]
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn unix_weekday(year: i32, month: u32, day: u32) -> u32 {
    // Zeller / Sakamoto: 0=Sunday
    let y = if month < 3 { year - 1 } else { year };
    let m = if month < 3 { month + 12 } else { month };
    let k = y % 100;
    let j = y / 100;
    let h = (day as i32 + (13 * (m + 1) as i32) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    h.rem_euclid(7) as u32
}

fn ymd_to_unix(year: i32, month: u32, day: u32, hour: u32) -> i64 {
    let mut days: i64 = 0;
    if year >= 1970 {
        for y in 1970..year {
            days += if is_leap_year(y) { 366 } else { 365 };
        }
    } else {
        for y in year..1970 {
            days -= if is_leap_year(y) { 366 } else { 365 };
        }
    }
    for m in 1..month {
        days += days_in_month(year, m) as i64;
    }
    days += day as i64 - 1;
    days * 86_400 + hour as i64 * 3600
}

fn is_us_dst(unix_sec: i64) -> bool {
    let (y, _, _, _, _, _, _) = unix_to_ymd_hms(unix_sec);
    let start = ymd_to_unix(y, 3, nth_weekday_of_month(y, 3, 0, 2), 2);
    let end = ymd_to_unix(y, 11, nth_weekday_of_month(y, 11, 0, 1), 2);
    unix_sec >= start && unix_sec < end
}

fn is_eu_dst(unix_sec: i64) -> bool {
    let (y, _, _, _, _, _, _) = unix_to_ymd_hms(unix_sec);
    let start = ymd_to_unix(y, 3, last_weekday_of_month(y, 3, 0), 1);
    let end = ymd_to_unix(y, 10, last_weekday_of_month(y, 10, 0), 1);
    unix_sec >= start && unix_sec < end
}

fn is_au_dst(unix_sec: i64) -> bool {
    // Australia/Sydney: first Sunday in October – first Sunday in April (approx).
    let (y, mo, _, _, _, _, _) = unix_to_ymd_hms(unix_sec);
    if mo >= 10 {
        let start = ymd_to_unix(y, 10, nth_weekday_of_month(y, 10, 0, 1), 2);
        unix_sec >= start
    } else if mo < 4 {
        let end = ymd_to_unix(y, 4, nth_weekday_of_month(y, 4, 0, 1), 3);
        unix_sec < end
    } else {
        false
    }
}

fn unix_to_ymd_hms(mut unix_sec: i64) -> (i32, u32, u32, u32, u32, u32, i64) {
    let mut days = unix_sec.div_euclid(86_400);
    let rem = unix_sec.rem_euclid(86_400);
    let hour = (rem / 3600) as u32;
    let minute = ((rem % 3600) / 60) as u32;
    let second = (rem % 60) as u32;
    let mut year: i32 = 1970;
    if days >= 0 {
        loop {
            let dy = if is_leap_year(year) { 366 } else { 365 };
            if days < dy {
                break;
            }
            days -= dy;
            year += 1;
        }
    } else {
        while days < 0 {
            year -= 1;
            days += if is_leap_year(year) { 366 } else { 365 };
        }
    }
    let mut month = 1u32;
    while month <= 12 {
        let dm = days_in_month(year, month) as i64;
        if days < dm {
            break;
        }
        days -= dm;
        month += 1;
    }
    let day = (days + 1) as u32;
    (year, month, day, hour, minute, second, rem)
}

fn embedded_is_dst(zone: &EmbeddedZone, unix_sec: i64) -> bool {
    match zone.dst_rule {
        DstRule::None => false,
        DstRule::Us => is_us_dst(unix_sec),
        DstRule::Eu => is_eu_dst(unix_sec),
        DstRule::Au => is_au_dst(unix_sec),
    }
}

/// Errors creating a [`TimeZone`] from TZif or zoneinfo data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeZoneError {
    /// TZif payload could not be parsed.
    InvalidTzif(TzifError),
    /// The zoneinfo file could not be read or mapped.
    FileError(MappedFileError),
}

/// A time zone (`GTimeZone`).
///
/// Supports UTC, fixed offsets, embedded IANA identifiers, and TZif-backed zones.
#[derive(Clone, Debug)]
pub struct TimeZone {
    identifier: String,
    offset_seconds: i32,
    is_dst: bool,
    embedded: Option<&'static EmbeddedZone>,
    tzif: Option<TzifData>,
}

impl TimeZone {
    /// Create a UTC time zone (`g_time_zone_new_utc`).
    pub fn utc() -> Self {
        Self {
            identifier: "UTC".to_owned(),
            offset_seconds: 0,
            is_dst: false,
            embedded: None,
            tzif: None,
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
            embedded: None,
            tzif: None,
        }
    }

    /// Create a time zone from an embedded IANA identifier (`g_time_zone_new_identifier`).
    ///
    /// Returns `None` if the name is not in the embedded table. Does not consult
    /// the filesystem.
    pub fn new_iana(name: &str) -> Option<Self> {
        let zone = lookup_embedded(name)?;
        Some(Self {
            identifier: name.to_owned(),
            offset_seconds: zone.std_offset,
            is_dst: false,
            embedded: Some(zone),
            tzif: None,
        })
    }

    /// Create a time zone from raw TZif bytes.
    pub fn from_tzif_bytes(data: &[u8]) -> Result<Self, TimeZoneError> {
        let tzif = TzifData::parse(data).map_err(TimeZoneError::InvalidTzif)?;
        let offset_seconds = tzif.offset_at(0);
        Ok(Self {
            identifier: tzif.abbreviation_at(0).to_owned(),
            offset_seconds,
            is_dst: tzif.is_dst_at(0),
            embedded: None,
            tzif: Some(tzif),
        })
    }

    /// Create a time zone by reading a zoneinfo file via the mapped-file platform.
    pub fn from_zoneinfo_file(path: &str) -> Result<Self, TimeZoneError> {
        let mapped = mapped_file_new(path, false).map_err(TimeZoneError::FileError)?;
        Self::from_tzif_bytes(mapped.get_contents())
    }

    /// Create a time zone from an identifier (`g_time_zone_new_identifier`).
    ///
    /// Supports "UTC", fixed offsets like "+05:30" or "-08:00", embedded IANA
    /// names, and falls back to UTC for unrecognized identifiers.
    pub fn new_identifier(identifier: &str) -> Self {
        if identifier == "UTC" || identifier == "Z" {
            return Self::utc();
        }

        if let Some(tz) = parse_offset(identifier) {
            return tz;
        }

        if let Some(tz) = Self::new_iana(identifier) {
            return tz;
        }

        Self::utc()
    }

    /// Get the identifier (`g_time_zone_get_identifier`).
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Offset in seconds at a Unix timestamp (embedded DST rules or TZif data).
    pub fn offset_at_unix(&self, unix_sec: i64) -> i32 {
        if let Some(tzif) = &self.tzif {
            return tzif.offset_at(unix_sec);
        }
        if let Some(zone) = self.embedded {
            if embedded_is_dst(zone, unix_sec) {
                zone.dst_offset
            } else {
                zone.std_offset
            }
        } else {
            self.offset_seconds
        }
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

    /// Check DST at a Unix timestamp for embedded or TZif-backed zones.
    pub fn is_dst_at_unix(&self, unix_sec: i64) -> bool {
        if let Some(tzif) = &self.tzif {
            return tzif.is_dst_at(unix_sec);
        }
        if let Some(zone) = self.embedded {
            embedded_is_dst(zone, unix_sec)
        } else {
            self.is_dst
        }
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
        let tz = TimeZone::new_identifier("Unknown/Zone");
        assert_eq!(tz.offset(0), 0);
    }

    #[test]
    fn iana_new_york_winter() {
        let tz = TimeZone::new_iana("America/New_York").unwrap();
        // 2024-01-15 12:00 UTC
        let unix = 1705312800i64;
        assert_eq!(tz.offset_at_unix(unix), -5 * 3600);
        assert!(!tz.is_dst_at_unix(unix));
    }

    #[test]
    fn iana_new_york_summer() {
        let tz = TimeZone::new_iana("America/New_York").unwrap();
        // 2024-07-15 12:00 UTC
        let unix = 1721044800i64;
        assert_eq!(tz.offset_at_unix(unix), -4 * 3600);
        assert!(tz.is_dst_at_unix(unix));
    }

    #[test]
    fn iana_identifier_lookup() {
        let tz = TimeZone::new_identifier("Europe/London");
        assert_eq!(tz.identifier(), "Europe/London");
        assert_eq!(tz.offset_at_unix(1704067200), 0); // 2024-01-01 UTC
    }

    #[test]
    fn iana_aliases() {
        assert!(TimeZone::new_iana("EST").is_some());
        assert!(TimeZone::new_iana("GMT").is_some());
        assert!(TimeZone::new_iana("Asia/Tokyo").is_some());
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

    #[test]
    fn from_tzif_utc() {
        let tz = TimeZone::from_tzif_bytes(&crate::tzif::fixture_utc_v1()).unwrap();
        assert_eq!(tz.offset_at_unix(0), 0);
        assert_eq!(tz.offset_at_unix(1_700_000_000), 0);
    }

    #[test]
    fn from_tzif_new_york_winter() {
        let tz = TimeZone::from_tzif_bytes(&crate::tzif::fixture_new_york_v1()).unwrap();
        assert_eq!(tz.offset_at_unix(1_200_398_400), -5 * 3600);
        assert!(!tz.is_dst_at_unix(1_200_398_400));
    }

    #[test]
    fn from_tzif_new_york_summer() {
        let tz = TimeZone::from_tzif_bytes(&crate::tzif::fixture_new_york_v1()).unwrap();
        assert_eq!(tz.offset_at_unix(1_184_500_800), -4 * 3600);
        assert!(tz.is_dst_at_unix(1_184_500_800));
    }

    #[test]
    fn from_tzif_invalid_bytes() {
        assert!(matches!(
            TimeZone::from_tzif_bytes(b"bad"),
            Err(TimeZoneError::InvalidTzif(TzifError::TooShort))
        ));
    }

    struct MockTzifPlatform;
    impl crate::mappedfile::MappedFilePlatform for MockTzifPlatform {
        fn open(
            &self,
            path: &str,
            writable: bool,
        ) -> Result<crate::mappedfile::MappedFile, MappedFileError> {
            use crate::mappedfile::MappedFile;
            if path == "/zoneinfo/UTC" {
                Ok(MappedFile::from_contents(
                    crate::tzif::fixture_utc_v1(),
                    writable,
                ))
            } else if path == "/zoneinfo/America/New_York" {
                Ok(MappedFile::from_contents(
                    crate::tzif::fixture_new_york_v1(),
                    writable,
                ))
            } else {
                Err(MappedFileError::NotFound)
            }
        }

        fn open_from_fd(
            &self,
            _fd: i32,
            _writable: bool,
        ) -> Result<crate::mappedfile::MappedFile, MappedFileError> {
            Err(MappedFileError::InvalidFd)
        }
    }

    #[test]
    fn from_zoneinfo_file_via_platform() {
        crate::mappedfile::register_mapped_file_platform(&MockTzifPlatform);
        let utc = TimeZone::from_zoneinfo_file("/zoneinfo/UTC").unwrap();
        assert_eq!(utc.offset_at_unix(0), 0);
        let ny = TimeZone::from_zoneinfo_file("/zoneinfo/America/New_York").unwrap();
        assert_eq!(ny.offset_at_unix(1_184_500_800), -4 * 3600);
        crate::mappedfile::register_mapped_file_platform(&crate::mappedfile::NoMappedFilePlatform);
    }

    #[test]
    fn embedded_still_works_with_tzif() {
        let embedded = TimeZone::new_iana("America/New_York").unwrap();
        assert_eq!(embedded.offset_at_unix(1705312800), -5 * 3600);
        let fixed = TimeZone::new_offset(-8 * 3600);
        assert_eq!(fixed.offset_at_unix(999_999_999), -8 * 3600);
    }
}
