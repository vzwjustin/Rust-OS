//! Date-time matching `gdatetime.h` / `gdatetime.c`.
//!
//! Immutable date-time with microsecond precision. The `new_now*` functions
//! require a platform clock source (deferred). All other operations
//! (construction from components, arithmetic, getters, formatting) are pure
//! computation. Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use crate::timezone::TimeZone;

/// Time span in microseconds (`GTimeSpan`).
pub type TimeSpan = i64;

/// One day in microseconds.
pub const TIME_SPAN_DAY: TimeSpan = 86_400_000_000;
/// One hour in microseconds.
pub const TIME_SPAN_HOUR: TimeSpan = 3_600_000_000;
/// One minute in microseconds.
pub const TIME_SPAN_MINUTE: TimeSpan = 60_000_000;
/// One second in microseconds.
pub const TIME_SPAN_SECOND: TimeSpan = 1_000_000;
/// One millisecond in microseconds.
pub const TIME_SPAN_MILLISECOND: TimeSpan = 1_000;

/// Days per month (non-leap year).
const DAYS_PER_MONTH: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    if month == 2 && is_leap_year(year) {
        return 29;
    }
    DAYS_PER_MONTH[(month - 1) as usize]
}

/// Convert (year, month, day, hour, min, sec, usec) to Unix timestamp in microseconds.
fn ymd_to_unix_usec(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    usec: u32,
) -> i64 {
    // Days since epoch (1970-01-01)
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
    days += (day as i64) - 1;

    days * TIME_SPAN_DAY
        + (hour as i64) * TIME_SPAN_HOUR
        + (minute as i64) * TIME_SPAN_MINUTE
        + (second as i64) * TIME_SPAN_SECOND
        + (usec as i64)
}

/// Convert Unix timestamp in microseconds to (year, month, day, hour, min, sec, usec).
fn unix_usec_to_ymd(mut usec: i64) -> (i32, u32, u32, u32, u32, u32, u32) {
    let mut days = usec.div_euclid(TIME_SPAN_DAY);
    let remainder = usec.rem_euclid(TIME_SPAN_DAY);
    usec = remainder;

    let hour = (usec / TIME_SPAN_HOUR) as u32;
    usec %= TIME_SPAN_HOUR;
    let minute = (usec / TIME_SPAN_MINUTE) as u32;
    usec %= TIME_SPAN_MINUTE;
    let second = (usec / TIME_SPAN_SECOND) as u32;
    let microsecond = (usec % TIME_SPAN_SECOND) as u32;

    // Now find year/month/day from days since epoch
    let mut year: i32 = 1970;
    if days >= 0 {
        loop {
            let dy = if is_leap_year(year) { 366 } else { 365 };
            if days < dy as i64 {
                break;
            }
            days -= dy as i64;
            year += 1;
        }
    } else {
        while days < 0 {
            year -= 1;
            let dy = if is_leap_year(year) { 366 } else { 365 };
            days += dy as i64;
        }
    }

    let mut month: u32 = 1;
    while month <= 12 {
        let dm = days_in_month(year, month) as i64;
        if days < dm {
            break;
        }
        days -= dm;
        month += 1;
    }
    let day = (days + 1) as u32;

    (year, month, day, hour, minute, second, microsecond)
}

/// An immutable date-time (`GDateTime`).
///
/// Stores time as Unix microseconds in UTC with an associated time zone for
/// local wall-clock display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateTime {
    usec: i64,
    timezone: TimeZone,
}

impl DateTime {
    fn from_usec_utc(usec: i64) -> Self {
        Self {
            usec,
            timezone: TimeZone::utc(),
        }
    }

    fn local_usec(&self) -> i64 {
        let offset = self
            .timezone
            .offset_at_unix(self.usec.div_euclid(TIME_SPAN_SECOND));
        self.usec + offset as i64 * TIME_SPAN_SECOND
    }
    /// Create from components, UTC (`g_date_time_new_utc`).
    pub fn new_utc(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        seconds: f64,
    ) -> Option<Self> {
        if month < 1 || month > 12 || day < 1 || day > days_in_month(year, month) {
            return None;
        }
        if hour > 23 || minute > 59 || seconds < 0.0 || seconds >= 60.0 {
            return None;
        }
        let sec = seconds as u32;
        let usec = ((seconds - sec as f64) * 1_000_000.0) as u32;
        let usec_total = ymd_to_unix_usec(year, month, day, hour, minute, sec, usec);
        Some(Self::from_usec_utc(usec_total))
    }

    /// Create from Unix timestamp, UTC (`g_date_time_new_from_unix_utc`).
    pub fn from_unix_utc(t: i64) -> Self {
        Self::from_usec_utc(t * TIME_SPAN_SECOND)
    }

    /// Create from Unix timestamp in microseconds, UTC (`g_date_time_new_from_unix_utc_usec`).
    pub fn from_unix_utc_usec(usec: i64) -> Self {
        Self::from_usec_utc(usec)
    }

    /// Convert to the same instant in another time zone (`g_date_time_to_timezone`).
    pub fn to_timezone(&self, tz: &TimeZone) -> Self {
        Self {
            usec: self.usec,
            timezone: tz.clone(),
        }
    }

    /// Get the associated time zone.
    pub fn timezone(&self) -> &TimeZone {
        &self.timezone
    }

    /// Convert to Unix timestamp (`g_date_time_to_unix`).
    pub fn to_unix(&self) -> i64 {
        self.usec / TIME_SPAN_SECOND
    }

    /// Convert to Unix timestamp in microseconds (`g_date_time_to_unix_usec`).
    pub fn to_unix_usec(&self) -> i64 {
        self.usec
    }

    /// Add a time span (`g_date_time_add`).
    pub fn add(&self, span: TimeSpan) -> Self {
        Self {
            usec: self.usec + span,
            timezone: self.timezone.clone(),
        }
    }

    /// Add years (`g_date_time_add_years`).
    pub fn add_years(&self, years: i32) -> Option<Self> {
        let (y, mo, d, h, mi, s, us) = unix_usec_to_ymd(self.usec);
        let new_year = y + years;
        // Clamp day if needed (e.g. Feb 29 -> Feb 28)
        let new_day = d.min(days_in_month(new_year, mo));
        Self::new_utc(
            new_year,
            mo,
            new_day,
            h,
            mi,
            s as f64 + us as f64 / 1_000_000.0,
        )
    }

    /// Add months (`g_date_time_add_months`).
    pub fn add_months(&self, months: i32) -> Option<Self> {
        let (y, mo, d, h, mi, s, us) = unix_usec_to_ymd(self.usec);
        let total_months = (y * 12 + mo as i32 - 1) + months;
        let new_year = total_months.div_euclid(12);
        let new_month = (total_months.rem_euclid(12) + 1) as u32;
        let new_day = d.min(days_in_month(new_year, new_month));
        Self::new_utc(
            new_year,
            new_month,
            new_day,
            h,
            mi,
            s as f64 + us as f64 / 1_000_000.0,
        )
    }

    /// Add weeks (`g_date_time_add_weeks`).
    pub fn add_weeks(&self, weeks: i32) -> Self {
        self.add(weeks as i64 * 7 * TIME_SPAN_DAY)
    }

    /// Add days (`g_date_time_add_days`).
    pub fn add_days(&self, days: i32) -> Self {
        self.add(days as i64 * TIME_SPAN_DAY)
    }

    /// Add hours (`g_date_time_add_hours`).
    pub fn add_hours(&self, hours: i32) -> Self {
        self.add(hours as i64 * TIME_SPAN_HOUR)
    }

    /// Add minutes (`g_date_time_add_minutes`).
    pub fn add_minutes(&self, minutes: i32) -> Self {
        self.add(minutes as i64 * TIME_SPAN_MINUTE)
    }

    /// Add seconds (`g_date_time_add_seconds`).
    pub fn add_seconds(&self, seconds: f64) -> Self {
        self.add((seconds * TIME_SPAN_SECOND as f64) as i64)
    }

    /// Get year (`g_date_time_get_year`).
    pub fn year(&self) -> i32 {
        unix_usec_to_ymd(self.local_usec()).0
    }

    /// Get month (`g_date_time_get_month`).
    pub fn month(&self) -> u32 {
        unix_usec_to_ymd(self.local_usec()).1
    }

    /// Get day of month (`g_date_time_get_day_of_month`).
    pub fn day_of_month(&self) -> u32 {
        unix_usec_to_ymd(self.local_usec()).2
    }

    /// Get hour (`g_date_time_get_hour`).
    pub fn hour(&self) -> u32 {
        unix_usec_to_ymd(self.local_usec()).3
    }

    /// Get minute (`g_date_time_get_minute`).
    pub fn minute(&self) -> u32 {
        unix_usec_to_ymd(self.local_usec()).4
    }

    /// Get second (`g_date_time_get_second`).
    pub fn second(&self) -> u32 {
        unix_usec_to_ymd(self.local_usec()).5
    }

    /// Get microsecond (`g_date_time_get_microsecond`).
    pub fn microsecond(&self) -> u32 {
        unix_usec_to_ymd(self.local_usec()).6
    }

    /// Get seconds as double (`g_date_time_get_seconds`).
    pub fn seconds(&self) -> f64 {
        let (_, _, _, _, _, s, us) = unix_usec_to_ymd(self.local_usec());
        s as f64 + us as f64 / 1_000_000.0
    }

    /// Get day of week (1=Monday..7=Sunday) (`g_date_time_get_day_of_week`).
    pub fn day_of_week(&self) -> u32 {
        let days = self.local_usec().div_euclid(TIME_SPAN_DAY);
        // 1970-01-01 was a Thursday (4)
        // Monday=1, so: ((days + 3) % 7) + 1
        ((days + 3).rem_euclid(7) + 1) as u32
    }

    /// Get day of year (1..366) (`g_date_time_get_day_of_year`).
    pub fn day_of_year(&self) -> u32 {
        let (y, mo, d, _, _, _, _) = unix_usec_to_ymd(self.local_usec());
        let mut doy = 0u32;
        for m in 1..mo {
            doy += days_in_month(y, m);
        }
        doy + d
    }

    /// Get week of year (`g_date_time_get_week_of_year`).
    pub fn week_of_year(&self) -> u32 {
        let doy = self.day_of_year() as i32;
        let dow = self.day_of_week() as i32; // 1=Mon..7=Sun
                                             // ISO 8601: week 1 is the week with the first Thursday
        let thu_doy = doy + (4 - dow);
        let week = (thu_doy + 6) / 7;
        if week < 1 {
            // Last week of previous year - approximate as 52 or 53
            let prev_year = self.year() - 1;
            if is_leap_year(prev_year) {
                53
            } else {
                52
            }
        } else if week > 52 && !is_leap_year(self.year()) && self.day_of_year() <= 365 {
            // Check if it should be week 1 of next year
            if dow > 4 {
                1
            } else {
                week as u32
            }
        } else {
            week as u32
        }
    }

    /// Get week numbering year (`g_date_time_get_week_numbering_year`).
    pub fn week_numbering_year(&self) -> i32 {
        let week = self.week_of_year();
        if week == 1 && self.day_of_year() > 7 {
            self.year() + 1
        } else if week >= 52 && self.day_of_year() <= 7 {
            self.year() - 1
        } else {
            self.year()
        }
    }

    /// Difference between two date-times (`g_date_time_difference`).
    pub fn difference(&self, other: &DateTime) -> TimeSpan {
        other.usec - self.usec
    }

    /// Compare two date-times (`g_date_time_compare`).
    pub fn compare(&self, other: &DateTime) -> core::cmp::Ordering {
        self.usec.cmp(&other.usec)
    }

    /// Format as ISO 8601 (`g_date_time_format_iso8601`).
    pub fn format_iso8601(&self) -> String {
        let (y, mo, d, h, mi, s, us) = unix_usec_to_ymd(self.local_usec());
        let offset = self
            .timezone
            .offset_at_unix(self.usec.div_euclid(TIME_SPAN_SECOND));
        let sign = if offset >= 0 { '+' } else { '-' };
        let abs = offset.unsigned_abs();
        let oh = abs / 3600;
        let om = (abs % 3600) / 60;
        let suffix = if offset == 0 {
            "Z".to_owned()
        } else {
            format!("{}{:02}:{:02}", sign, oh, om)
        };
        if us > 0 {
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}{}",
                y, mo, d, h, mi, s, us, suffix
            )
        } else {
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}",
                y, mo, d, h, mi, s, suffix
            )
        }
    }

    /// Format with a simple format string (`g_date_time_format`).
    ///
    /// Supports: %Y, %m, %d, %H, %M, %S, %j, %u, %U, %V, %z, %Z, %%.
    pub fn format(&self, fmt: &str) -> String {
        let (y, mo, d, h, mi, s, us) = unix_usec_to_ymd(self.local_usec());
        let offset = self
            .timezone
            .offset_at_unix(self.usec.div_euclid(TIME_SPAN_SECOND));
        let mut result = String::new();
        let mut chars = fmt.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%' {
                match chars.next() {
                    Some('Y') => {
                        let _ = write!(result, "{:04}", y);
                    }
                    Some('m') => {
                        let _ = write!(result, "{:02}", mo);
                    }
                    Some('d') => {
                        let _ = write!(result, "{:02}", d);
                    }
                    Some('H') => {
                        let _ = write!(result, "{:02}", h);
                    }
                    Some('M') => {
                        let _ = write!(result, "{:02}", mi);
                    }
                    Some('S') => {
                        let _ = write!(result, "{:02}", s);
                    }
                    Some('j') => {
                        let _ = write!(result, "{:03}", self.day_of_year());
                    }
                    Some('u') => {
                        let _ = write!(result, "{}", self.day_of_week());
                    }
                    Some('U') => {
                        let _ = write!(result, "{:02}", self.week_of_year());
                    }
                    Some('V') => {
                        let _ = write!(result, "{:02}", self.week_of_year());
                    }
                    Some('f') => {
                        let _ = write!(result, "{:06}", us);
                    }
                    Some('z') => {
                        let sign = if offset >= 0 { '+' } else { '-' };
                        let abs = offset.unsigned_abs();
                        let _ = write!(result, "{}{:02}{:02}", sign, abs / 3600, (abs % 3600) / 60);
                    }
                    Some('Z') => {
                        result.push_str(self.timezone.identifier());
                    }
                    Some('%') => result.push('%'),
                    Some(other) => {
                        result.push('%');
                        result.push(other);
                    }
                    None => result.push('%'),
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Get YMD (`g_date_time_get_ymd`).
    pub fn ymd(&self) -> (i32, u32, u32) {
        let (y, mo, d, _, _, _, _) = unix_usec_to_ymd(self.local_usec());
        (y, mo, d)
    }

    /// UTC offset (`g_date_time_get_utc_offset`).
    pub fn utc_offset(&self) -> TimeSpan {
        self.timezone
            .offset_at_unix(self.usec.div_euclid(TIME_SPAN_SECOND)) as i64
            * TIME_SPAN_SECOND
    }

    /// Is daylight savings (`g_date_time_is_daylight_savings`).
    pub fn is_daylight_savings(&self) -> bool {
        self.timezone
            .is_dst_at_unix(self.usec.div_euclid(TIME_SPAN_SECOND))
    }
}

impl PartialOrd for DateTime {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DateTime {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.usec.cmp(&other.usec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timezone::TimeZone;

    #[test]
    fn new_utc_basic() {
        let dt = DateTime::new_utc(2024, 1, 1, 0, 0, 0.0).unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day_of_month(), 1);
        assert_eq!(dt.hour(), 0);
        assert_eq!(dt.to_unix(), 1704067200);
    }

    #[test]
    fn from_unix() {
        let dt = DateTime::from_unix_utc(0);
        assert_eq!(dt.year(), 1970);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day_of_month(), 1);
        assert_eq!(dt.hour(), 0);
    }

    #[test]
    fn to_unix_roundtrip() {
        let dt = DateTime::new_utc(2024, 6, 15, 12, 30, 45.0).unwrap();
        let unix = dt.to_unix();
        let dt2 = DateTime::from_unix_utc(unix);
        assert_eq!(dt.year(), dt2.year());
        assert_eq!(dt.month(), dt2.month());
        assert_eq!(dt.day_of_month(), dt2.day_of_month());
        assert_eq!(dt.hour(), dt2.hour());
        assert_eq!(dt.minute(), dt2.minute());
        assert_eq!(dt.second(), dt2.second());
    }

    #[test]
    fn add_days() {
        let dt = DateTime::new_utc(2024, 1, 1, 0, 0, 0.0).unwrap();
        let dt2 = dt.add_days(31);
        assert_eq!(dt2.month(), 2);
        assert_eq!(dt2.day_of_month(), 1);
    }

    #[test]
    fn add_months() {
        let dt = DateTime::new_utc(2024, 1, 31, 0, 0, 0.0).unwrap();
        let dt2 = dt.add_months(1).unwrap();
        // Jan 31 + 1 month = Feb 29 (leap year) or Feb 28
        assert_eq!(dt2.month(), 2);
        assert_eq!(dt2.day_of_month(), 29); // 2024 is a leap year
    }

    #[test]
    fn add_years() {
        let dt = DateTime::new_utc(2024, 2, 29, 12, 0, 0.0).unwrap();
        let dt2 = dt.add_years(1).unwrap();
        // 2025 is not a leap year, so Feb 29 -> Feb 28
        assert_eq!(dt2.year(), 2025);
        assert_eq!(dt2.month(), 2);
        assert_eq!(dt2.day_of_month(), 28);
    }

    #[test]
    fn day_of_week() {
        // 1970-01-01 was Thursday (4)
        let dt = DateTime::from_unix_utc(0);
        assert_eq!(dt.day_of_week(), 4);
        // 2024-01-01 was Monday (1)
        let dt2 = DateTime::new_utc(2024, 1, 1, 0, 0, 0.0).unwrap();
        assert_eq!(dt2.day_of_week(), 1);
    }

    #[test]
    fn day_of_year() {
        let dt = DateTime::new_utc(2024, 3, 1, 0, 0, 0.0).unwrap();
        // Jan(31) + Feb(29, leap) + 1 = 61
        assert_eq!(dt.day_of_year(), 61);
    }

    #[test]
    fn format_iso8601() {
        let dt = DateTime::new_utc(2024, 6, 15, 12, 30, 45.0).unwrap();
        assert_eq!(dt.format_iso8601(), "2024-06-15T12:30:45Z");
    }

    #[test]
    fn format_custom() {
        let dt = DateTime::new_utc(2024, 6, 15, 12, 30, 45.0).unwrap();
        assert_eq!(dt.format("%Y-%m-%d %H:%M:%S"), "2024-06-15 12:30:45");
    }

    #[test]
    fn difference() {
        let a = DateTime::new_utc(2024, 1, 1, 0, 0, 0.0).unwrap();
        let b = DateTime::new_utc(2024, 1, 2, 0, 0, 0.0).unwrap();
        assert_eq!(a.difference(&b), TIME_SPAN_DAY);
    }

    #[test]
    fn compare() {
        let a = DateTime::new_utc(2024, 1, 1, 0, 0, 0.0).unwrap();
        let b = DateTime::new_utc(2024, 1, 2, 0, 0, 0.0).unwrap();
        assert!(a < b);
        assert!(b > a);
        assert_eq!(a.compare(&a), core::cmp::Ordering::Equal);
    }

    #[test]
    fn microseconds() {
        let dt = DateTime::new_utc(2024, 1, 1, 0, 0, 0.5).unwrap();
        assert_eq!(dt.second(), 0);
        assert_eq!(dt.microsecond(), 500_000);
    }

    #[test]
    fn invalid_dates() {
        assert!(DateTime::new_utc(2024, 13, 1, 0, 0, 0.0).is_none());
        assert!(DateTime::new_utc(2024, 2, 30, 0, 0, 0.0).is_none());
        assert!(DateTime::new_utc(2024, 1, 1, 25, 0, 0.0).is_none());
    }

    #[test]
    fn pre_epoch() {
        let dt = DateTime::from_unix_utc(-1);
        assert_eq!(dt.year(), 1969);
        assert_eq!(dt.month(), 12);
        assert_eq!(dt.day_of_month(), 31);
        assert_eq!(dt.hour(), 23);
        assert_eq!(dt.minute(), 59);
        assert_eq!(dt.second(), 59);
    }

    #[test]
    fn to_timezone_new_york() {
        let dt = DateTime::new_utc(2024, 1, 15, 12, 0, 0.0).unwrap();
        let tz = TimeZone::new_iana("America/New_York").unwrap();
        let local = dt.to_timezone(&tz);
        assert_eq!(local.to_unix(), dt.to_unix());
        assert_eq!(local.hour(), 7); // EST = UTC-5
        assert_eq!(local.utc_offset(), -5 * TIME_SPAN_HOUR);
    }

    #[test]
    fn to_timezone_london_summer() {
        let dt = DateTime::new_utc(2024, 7, 15, 12, 0, 0.0).unwrap();
        let tz = TimeZone::new_iana("Europe/London").unwrap();
        let local = dt.to_timezone(&tz);
        assert_eq!(local.hour(), 13); // BST = UTC+1
        assert!(local.is_daylight_savings());
    }

    #[test]
    fn format_with_timezone() {
        let dt = DateTime::new_utc(2024, 6, 15, 12, 30, 45.0).unwrap();
        let tz = TimeZone::new_offset(5 * 3600 + 30 * 60);
        let local = dt.to_timezone(&tz);
        assert_eq!(local.format("%H:%M:%S"), "18:00:45");
        assert_eq!(local.format("%z"), "+0530");
    }
}
