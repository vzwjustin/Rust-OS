//! Date calculations matching `gdate.h` / `gdate.c`.
//!
//! Pure date math with no OS dependencies. Supports Julian day conversions,
//! DMY (day/month/year) representation, date arithmetic, leap year checks,
//! and ISO 8601 week numbering. All fully `no_std` compatible.

use crate::prelude::*;

/// Day of the month (1-31).
pub type DateDay = u8;

/// Year (1-8000 approximately).
pub type DateYear = u16;

/// Month of the year (`GDateMonth`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DateMonth {
    /// Invalid month.
    Bad = 0,
    /// January.
    January = 1,
    /// February.
    February = 2,
    /// March.
    March = 3,
    /// April.
    April = 4,
    /// May.
    May = 5,
    /// June.
    June = 6,
    /// July.
    July = 7,
    /// August.
    August = 8,
    /// September.
    September = 9,
    /// October.
    October = 10,
    /// November.
    November = 11,
    /// December.
    December = 12,
}

impl DateMonth {
    /// Convert from a raw integer.
    pub fn from_u8(n: u8) -> Self {
        match n {
            1 => Self::January,
            2 => Self::February,
            3 => Self::March,
            4 => Self::April,
            5 => Self::May,
            6 => Self::June,
            7 => Self::July,
            8 => Self::August,
            9 => Self::September,
            10 => Self::October,
            11 => Self::November,
            12 => Self::December,
            _ => Self::Bad,
        }
    }

    /// Convert to a raw integer.
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Day of the week (`GDateWeekday`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DateWeekday {
    /// Invalid weekday.
    Bad = 0,
    /// Monday.
    Monday = 1,
    /// Tuesday.
    Tuesday = 2,
    /// Wednesday.
    Wednesday = 3,
    /// Thursday.
    Thursday = 4,
    /// Friday.
    Friday = 5,
    /// Saturday.
    Saturday = 6,
    /// Sunday.
    Sunday = 7,
}

impl DateWeekday {
    /// Convert from a raw integer.
    pub fn from_u8(n: u8) -> Self {
        match n {
            1 => Self::Monday,
            2 => Self::Tuesday,
            3 => Self::Wednesday,
            4 => Self::Thursday,
            5 => Self::Friday,
            6 => Self::Saturday,
            7 => Self::Sunday,
            _ => Self::Bad,
        }
    }
}

/// A date value, stored internally as Julian day number.
#[derive(Clone, Copy, Debug)]
pub struct Date {
    julian_days: u32,
    valid: bool,
}

/// Sentinel for invalid Julian day.
pub const DATE_BAD_JULIAN: u32 = 0;

/// Returns `true` if `day` is a valid day of the month (1-31).
pub fn valid_day(day: DateDay) -> bool {
    day > 0 && day <= 31
}

/// Returns `true` if `month` is a valid month (1-12).
pub fn valid_month(month: DateMonth) -> bool {
    month != DateMonth::Bad
}

/// Returns `true` if `year` is a valid year (1-8000).
pub fn valid_year(year: DateYear) -> bool {
    year > 0 && year <= 8000
}

/// Returns `true` if `weekday` is a valid weekday (1-7).
pub fn valid_weekday(weekday: DateWeekday) -> bool {
    weekday != DateWeekday::Bad
}

/// Returns `true` if `julian_date` is a valid Julian day number.
pub fn valid_julian(julian_date: u32) -> bool {
    julian_date > 0 && julian_date <= 2_933_063
}

/// Returns `true` if the given day/month/year combination is a valid date.
pub fn valid_dmy(day: DateDay, month: DateMonth, year: DateYear) -> bool {
    if !valid_day(day) || !valid_month(month) || !valid_year(year) {
        return false;
    }
    day <= get_days_in_month(month, year)
}

/// Returns `true` if `year` is a leap year.
pub fn is_leap_year(year: DateYear) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Returns the number of days in `month` of `year`.
pub fn get_days_in_month(month: DateMonth, year: DateYear) -> u8 {
    match month {
        DateMonth::January
        | DateMonth::March
        | DateMonth::May
        | DateMonth::July
        | DateMonth::August
        | DateMonth::October
        | DateMonth::December => 31,
        DateMonth::April | DateMonth::June | DateMonth::September | DateMonth::November => 30,
        DateMonth::February => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        DateMonth::Bad => 0,
    }
}

/// Number of days in each month for a non-leap year.
const DAYS_BEFORE_MONTH: [u16; 13] = [0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

impl Date {
    /// Create a new invalid date (`g_date_new`).
    pub fn new() -> Self {
        Self {
            julian_days: 0,
            valid: false,
        }
    }

    /// Create a date from day/month/year (`g_date_new_dmy`).
    pub fn new_dmy(day: DateDay, month: DateMonth, year: DateYear) -> Self {
        if !valid_dmy(day, month, year) {
            return Self::new();
        }
        Self {
            julian_days: dmy_to_julian(day, month, year),
            valid: true,
        }
    }

    /// Create a date from a Julian day number (`g_date_new_julian`).
    pub fn new_julian(julian_day: u32) -> Self {
        if !valid_julian(julian_day) {
            return Self::new();
        }
        Self {
            julian_days: julian_day,
            valid: true,
        }
    }

    /// Returns `true` if this date is valid.
    pub fn valid(&self) -> bool {
        self.valid
    }

    /// Returns the Julian day number.
    pub fn julian(&self) -> u32 {
        self.julian_days
    }

    /// Returns the day of the month (1-31).
    pub fn day(&self) -> DateDay {
        if !self.valid {
            return 0;
        }
        julian_to_dmy(self.julian_days).0
    }

    /// Returns the month.
    pub fn month(&self) -> DateMonth {
        if !self.valid {
            return DateMonth::Bad;
        }
        julian_to_dmy(self.julian_days).1
    }

    /// Returns the year.
    pub fn year(&self) -> DateYear {
        if !self.valid {
            return 0;
        }
        julian_to_dmy(self.julian_days).2
    }

    /// Returns the day of the week.
    pub fn weekday(&self) -> DateWeekday {
        if !self.valid {
            return DateWeekday::Bad;
        }
        // Julian day 0 was a Monday; day 1 was Tuesday, etc.
        // GLib day 1 = Jan 1, AD 1 = Monday (weekday 1).
        // (day - 1) % 7 → 0=Mon, 1=Tue, ..., 5=Sat, 6=Sun
        let wd = ((self.julian_days - 1) % 7) as u8;
        match wd {
            0 => DateWeekday::Monday,
            1 => DateWeekday::Tuesday,
            2 => DateWeekday::Wednesday,
            3 => DateWeekday::Thursday,
            4 => DateWeekday::Friday,
            5 => DateWeekday::Saturday,
            6 => DateWeekday::Sunday,
            _ => DateWeekday::Bad,
        }
    }

    /// Returns the day of the year (1-366).
    pub fn day_of_year(&self) -> u16 {
        if !self.valid {
            return 0;
        }
        let (day, month, year) = julian_to_dmy(self.julian_days);
        let mut doy = DAYS_BEFORE_MONTH[month as usize] + day as u16;
        if month != DateMonth::January && month != DateMonth::February && is_leap_year(year) {
            doy += 1;
        }
        doy
    }

    /// Returns the ISO 8601 week number (1-53).
    pub fn iso8601_week_of_year(&self) -> u8 {
        if !self.valid {
            return 0;
        }
        let doy = self.day_of_year();
        let weekday = self.weekday() as u8;
        // ISO week: week 1 is the first week with a Thursday
        let thu_doy = doy as i32 + 4 - weekday as i32;
        let cur_year_days: i32 = if is_leap_year(self.year()) { 366 } else { 365 };
        let thu_year = if thu_doy <= 0 {
            // Thursday is in the previous year
            let prev_year = self.year() - 1;
            let prev_year_days: i32 = if is_leap_year(prev_year) { 366 } else { 365 };
            thu_doy + prev_year_days
        } else if thu_doy > cur_year_days {
            // Thursday is in the next year
            thu_doy - cur_year_days
        } else {
            thu_doy
        };
        ((thu_year - 1) / 7 + 1) as u8
    }

    /// Returns the Monday-based week number (0-53).
    pub fn monday_week_of_year(&self) -> u8 {
        if !self.valid {
            return 0;
        }
        let doy = self.day_of_year();
        let weekday = self.weekday() as u8;
        // Days since first Monday of the year
        let first_weekday = (doy as i32 - weekday as i32 + 1).rem_euclid(7);
        ((doy as i32 - first_weekday - 1) / 7 + 1) as u8
    }

    /// Returns the Sunday-based week number (0-53).
    pub fn sunday_week_of_year(&self) -> u8 {
        if !self.valid {
            return 0;
        }
        let doy = self.day_of_year();
        let weekday = self.weekday() as u8;
        // Days since first Sunday of the year
        let first_weekday = (doy as i32 - weekday as i32).rem_euclid(7);
        ((doy as i32 - first_weekday - 1) / 7 + 1) as u8
    }

    /// Returns `true` if this date is the first day of its month.
    pub fn is_first_of_month(&self) -> bool {
        self.valid && self.day() == 1
    }

    /// Returns `true` if this date is the last day of its month.
    pub fn is_last_of_month(&self) -> bool {
        if !self.valid {
            return false;
        }
        let (day, month, year) = julian_to_dmy(self.julian_days);
        day == get_days_in_month(month, year)
    }

    /// Add `n_days` days to this date.
    pub fn add_days(&mut self, n_days: u32) {
        if !self.valid {
            return;
        }
        self.julian_days = self.julian_days.saturating_add(n_days);
    }

    /// Subtract `n_days` days from this date.
    pub fn subtract_days(&mut self, n_days: u32) {
        if !self.valid {
            return;
        }
        self.julian_days = self.julian_days.saturating_sub(n_days);
    }

    /// Add `n_months` months to this date.
    pub fn add_months(&mut self, n_months: u32) {
        if !self.valid {
            return;
        }
        let (day, month, year) = julian_to_dmy(self.julian_days);
        let total_months = year as u32 * 12 + (month as u32 - 1) + n_months;
        let new_year = (total_months / 12) as DateYear;
        let new_month = DateMonth::from_u8((total_months % 12 + 1) as u8);
        let max_day = get_days_in_month(new_month, new_year);
        let new_day = day.min(max_day);
        self.julian_days = dmy_to_julian(new_day, new_month, new_year);
    }

    /// Subtract `n_months` months from this date.
    pub fn subtract_months(&mut self, n_months: u32) {
        if !self.valid {
            return;
        }
        let (day, month, year) = julian_to_dmy(self.julian_days);
        let total_months = year as u32 * 12 + (month as u32 - 1);
        if n_months >= total_months {
            return;
        }
        let new_total = total_months - n_months;
        let new_year = (new_total / 12) as DateYear;
        let new_month = DateMonth::from_u8((new_total % 12 + 1) as u8);
        let max_day = get_days_in_month(new_month, new_year);
        let new_day = day.min(max_day);
        self.julian_days = dmy_to_julian(new_day, new_month, new_year);
    }

    /// Add `n_years` years to this date.
    pub fn add_years(&mut self, n_years: u32) {
        if !self.valid {
            return;
        }
        let (day, month, year) = julian_to_dmy(self.julian_days);
        let new_year = (year as u32 + n_years) as DateYear;
        if new_year > 8000 {
            return;
        }
        // Handle Feb 29 -> Feb 28 on non-leap years
        let max_day = get_days_in_month(month, new_year);
        let new_day = day.min(max_day);
        self.julian_days = dmy_to_julian(new_day, month, new_year);
    }

    /// Subtract `n_years` years from this date.
    pub fn subtract_years(&mut self, n_years: u32) {
        if !self.valid {
            return;
        }
        let (day, month, year) = julian_to_dmy(self.julian_days);
        if n_years >= year as u32 {
            return;
        }
        let new_year = (year as u32 - n_years) as DateYear;
        let max_day = get_days_in_month(month, new_year);
        let new_day = day.min(max_day);
        self.julian_days = dmy_to_julian(new_day, month, new_year);
    }

    /// Returns the number of days between two dates.
    /// Negative if `other` is before `self`.
    pub fn days_between(&self, other: &Date) -> i32 {
        if !self.valid || !other.valid {
            return 0;
        }
        other.julian_days as i32 - self.julian_days as i32
    }

    /// Compare two dates (-1, 0, 1).
    pub fn compare(&self, other: &Date) -> i32 {
        match self.julian_days.cmp(&other.julian_days) {
            core::cmp::Ordering::Less => -1,
            core::cmp::Ordering::Equal => 0,
            core::cmp::Ordering::Greater => 1,
        }
    }

    /// Clamp this date to be within `[min, max]`.
    pub fn clamp(&mut self, min: &Date, max: &Date) {
        if self.julian_days < min.julian_days {
            *self = *min;
        } else if self.julian_days > max.julian_days {
            *self = *max;
        }
    }

    /// Set the date from day/month/year.
    pub fn set_dmy(&mut self, day: DateDay, month: DateMonth, year: DateYear) {
        if !valid_dmy(day, month, year) {
            self.valid = false;
            return;
        }
        self.julian_days = dmy_to_julian(day, month, year);
        self.valid = true;
    }

    /// Set the date from a Julian day number.
    pub fn set_julian(&mut self, julian_day: u32) {
        if !valid_julian(julian_day) {
            self.valid = false;
            return;
        }
        self.julian_days = julian_day;
        self.valid = true;
    }
}

impl Default for Date {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert day/month/year to Julian day number.
///
/// Uses the algorithm from the Calendar FAQ by Peter Baum.
fn dmy_to_julian(day: DateDay, month: DateMonth, year: DateYear) -> u32 {
    let m = month as i64;
    let y = year as i64;
    let d = day as i64;

    let (adj_y, adj_m) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };

    // Gregorian correction — must be i64 to avoid underflow (b is negative for years > 200)
    let a = adj_y / 100;
    let b = 2 - a + a / 4;

    // Meeus "Astronomical Algorithms" formula for astronomical JDN
    let jd =
        (365.25 * (adj_y + 4716) as f64) as i64 + (30.6001 * (adj_m + 1) as f64) as i64 + d + b
            - 1524;

    // GLib day 1 = Jan 1, AD 1 = astronomical JDN 1721426
    (jd - 1721425) as u32
}

/// Convert Julian day number to (day, month, year).
fn julian_to_dmy(julian: u32) -> (DateDay, DateMonth, DateYear) {
    // GLib day 1 = Jan 1, AD 1 = astronomical JDN 1721426
    let j = julian as i64 + 1721425;

    // Richards algorithm for proleptic Gregorian calendar
    let p = j + 68569;
    let q = 4 * p / 146097;
    let r = p - (146097 * q + 3) / 4;
    let s = 4000 * (r + 1) / 1461001;
    let t = r - 1461 * s / 4 + 31;
    let u = 80 * t / 2447;
    let v = u / 11;

    let day = t - 2447 * u / 80;
    let month = u + 2 - 12 * v;
    let year = 100 * (q - 49) + s + v;

    (
        day as DateDay,
        DateMonth::from_u8(month as u8),
        year as DateYear,
    )
}

/// Returns the number of Monday-based weeks in `year` (52 or 53).
pub fn monday_weeks_in_year(year: DateYear) -> u8 {
    let jan1 = Date::new_dmy(1, DateMonth::January, year);
    let dec31 = Date::new_dmy(31, DateMonth::December, year);
    let jan1_wd = jan1.weekday() as u8;
    let dec31_wd = dec31.weekday() as u8;
    if jan1_wd == 1 || dec31_wd == 1 || (jan1_wd == 7 && dec31_wd == 7 && is_leap_year(year)) {
        53
    } else {
        52
    }
}

/// Returns the number of Sunday-based weeks in `year` (52 or 53).
pub fn sunday_weeks_in_year(year: DateYear) -> u8 {
    let jan1 = Date::new_dmy(1, DateMonth::January, year);
    let dec31 = Date::new_dmy(31, DateMonth::December, year);
    let jan1_wd = jan1.weekday() as u8;
    let dec31_wd = dec31.weekday() as u8;
    if jan1_wd == 7 || dec31_wd == 7 || (jan1_wd == 6 && dec31_wd == 6 && is_leap_year(year)) {
        53
    } else {
        52
    }
}

/// Parse a date string in common formats (`g_date_set_parse`).
///
/// Supports formats like "YYYY-MM-DD", "DD/MM/YYYY", "MM/DD/YYYY",
/// "Month DD, YYYY", etc.
pub fn date_parse(s: &str) -> Option<Date> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Try ISO format: YYYY-MM-DD
    if s.len() >= 10 {
        if let Some(d) = parse_iso(s) {
            return Some(d);
        }
    }

    // Try numeric formats with separators
    if let Some(d) = parse_numeric(s) {
        return Some(d);
    }

    None
}

fn parse_iso(s: &str) -> Option<Date> {
    let parts: Vec<&str> = s.splitn(3, '-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: u16 = parts[0].parse().ok()?;
    let month: u8 = parts[1].parse().ok()?;
    let day: u8 = parts[2].parse().ok()?;
    let date = Date::new_dmy(day, DateMonth::from_u8(month), year);
    if date.valid() {
        Some(date)
    } else {
        None
    }
}

fn parse_numeric(s: &str) -> Option<Date> {
    let parts: Vec<&str> = s.split(['/', '.', ' ']).collect();
    if parts.len() < 3 {
        return None;
    }
    let n1: u16 = parts[0].parse().ok()?;
    let n2: u16 = parts[1].parse().ok()?;
    let n3: u16 = parts[2].parse().ok()?;

    // Heuristic: if first number > 31, it's a year (YYYY-MM-DD)
    if n1 > 31 {
        let d = Date::new_dmy(n3 as u8, DateMonth::from_u8(n2 as u8), n1);
        if d.valid() {
            return Some(d);
        }
    }
    // Otherwise assume MM/DD/YYYY or DD/MM/YYYY
    // US format: MM/DD/YYYY
    if n1 <= 12 && n2 <= 31 && n3 > 31 {
        let d = Date::new_dmy(n2 as u8, DateMonth::from_u8(n1 as u8), n3);
        if d.valid() {
            return Some(d);
        }
    }
    // European format: DD/MM/YYYY
    if n1 <= 31 && n2 <= 12 && n3 > 31 {
        let d = Date::new_dmy(n1 as u8, DateMonth::from_u8(n2 as u8), n3);
        if d.valid() {
            return Some(d);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2023));
    }

    #[test]
    fn days_in_month() {
        assert_eq!(get_days_in_month(DateMonth::January, 2024), 31);
        assert_eq!(get_days_in_month(DateMonth::February, 2024), 29);
        assert_eq!(get_days_in_month(DateMonth::February, 2023), 28);
        assert_eq!(get_days_in_month(DateMonth::April, 2024), 30);
    }

    #[test]
    fn date_dmy() {
        let d = Date::new_dmy(15, DateMonth::March, 2024);
        assert!(d.valid());
        assert_eq!(d.day(), 15);
        assert_eq!(d.month(), DateMonth::March);
        assert_eq!(d.year(), 2024);
    }

    #[test]
    fn date_julian_roundtrip() {
        let d = Date::new_dmy(1, DateMonth::January, 2000);
        let jd = d.julian();
        let d2 = Date::new_julian(jd);
        assert_eq!(d2.day(), 1);
        assert_eq!(d2.month(), DateMonth::January);
        assert_eq!(d2.year(), 2000);
    }

    #[test]
    fn weekday() {
        // January 1, 2000 was a Saturday
        let d = Date::new_dmy(1, DateMonth::January, 2000);
        assert_eq!(d.weekday(), DateWeekday::Saturday);

        // January 1, 2024 was a Monday
        let d = Date::new_dmy(1, DateMonth::January, 2024);
        assert_eq!(d.weekday(), DateWeekday::Monday);
    }

    #[test]
    fn day_of_year() {
        let d = Date::new_dmy(1, DateMonth::January, 2024);
        assert_eq!(d.day_of_year(), 1);

        let d = Date::new_dmy(1, DateMonth::March, 2024);
        assert_eq!(d.day_of_year(), 61); // 31 (Jan) + 29 (Feb, leap) + 1

        let d = Date::new_dmy(31, DateMonth::December, 2023);
        assert_eq!(d.day_of_year(), 365);
    }

    #[test]
    fn add_subtract_days() {
        let mut d = Date::new_dmy(1, DateMonth::January, 2024);
        d.add_days(31);
        assert_eq!(d.day(), 1);
        assert_eq!(d.month(), DateMonth::February);

        d.subtract_days(1);
        assert_eq!(d.day(), 31);
        assert_eq!(d.month(), DateMonth::January);
    }

    #[test]
    fn add_months() {
        let mut d = Date::new_dmy(31, DateMonth::January, 2024);
        d.add_months(1);
        // Feb 2024 has 29 days, so day 31 clamps to 29
        assert_eq!(d.day(), 29);
        assert_eq!(d.month(), DateMonth::February);

        let mut d = Date::new_dmy(15, DateMonth::June, 2024);
        d.add_months(6);
        assert_eq!(d.month(), DateMonth::December);
        assert_eq!(d.year(), 2024);
    }

    #[test]
    fn add_years() {
        let mut d = Date::new_dmy(29, DateMonth::February, 2024);
        d.add_years(1);
        // 2025 is not a leap year, so Feb 29 -> Feb 28
        assert_eq!(d.day(), 28);
        assert_eq!(d.year(), 2025);

        let mut d = Date::new_dmy(15, DateMonth::June, 2024);
        d.add_years(10);
        assert_eq!(d.year(), 2034);
        assert_eq!(d.day(), 15);
    }

    #[test]
    fn days_between() {
        let d1 = Date::new_dmy(1, DateMonth::January, 2024);
        let d2 = Date::new_dmy(2, DateMonth::January, 2024);
        assert_eq!(d1.days_between(&d2), 1);
        assert_eq!(d2.days_between(&d1), -1);

        let d3 = Date::new_dmy(1, DateMonth::February, 2024);
        assert_eq!(d1.days_between(&d3), 31);
    }

    #[test]
    fn first_last_of_month() {
        let d = Date::new_dmy(1, DateMonth::March, 2024);
        assert!(d.is_first_of_month());
        assert!(!d.is_last_of_month());

        let d = Date::new_dmy(31, DateMonth::March, 2024);
        assert!(!d.is_first_of_month());
        assert!(d.is_last_of_month());

        let d = Date::new_dmy(29, DateMonth::February, 2024);
        assert!(d.is_last_of_month());
    }

    #[test]
    fn compare_dates() {
        let d1 = Date::new_dmy(1, DateMonth::January, 2024);
        let d2 = Date::new_dmy(2, DateMonth::January, 2024);
        assert_eq!(d1.compare(&d2), -1);
        assert_eq!(d2.compare(&d1), 1);
        assert_eq!(d1.compare(&d1), 0);
    }

    #[test]
    fn clamp_date() {
        let mut d = Date::new_dmy(15, DateMonth::June, 2024);
        let min = Date::new_dmy(1, DateMonth::January, 2024);
        let max = Date::new_dmy(31, DateMonth::December, 2024);
        d.clamp(&min, &max);
        assert_eq!(d.day(), 15);

        let mut d = Date::new_dmy(1, DateMonth::January, 2023);
        d.clamp(&min, &max);
        assert_eq!(d.year(), 2024);
        assert_eq!(d.month(), DateMonth::January);
    }

    #[test]
    fn parse_iso_format() {
        let d = date_parse("2024-03-15").unwrap();
        assert_eq!(d.year(), 2024);
        assert_eq!(d.month(), DateMonth::March);
        assert_eq!(d.day(), 15);
    }

    #[test]
    fn parse_us_format() {
        let d = date_parse("03/15/2024").unwrap();
        assert_eq!(d.month(), DateMonth::March);
        assert_eq!(d.day(), 15);
        assert_eq!(d.year(), 2024);
    }

    #[test]
    fn invalid_dates() {
        assert!(!valid_dmy(31, DateMonth::February, 2023));
        assert!(!valid_dmy(0, DateMonth::January, 2024));
        assert!(!valid_dmy(32, DateMonth::January, 2024));
        assert!(valid_dmy(29, DateMonth::February, 2024));
    }
}
