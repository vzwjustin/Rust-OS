//! Real-Time Clock (RTC) driver for RustOS
//!
//! Provides access to Motorola MC146818-compatible CMOS RTC hardware.  The
//! conversion and validation paths mirror the common Linux RTC/CMOS handling:
//! wait for update-in-progress to clear, sample stable CMOS fields, honor the
//! data-mode bits in Status Register B, convert BCD/binary and 12h/24h hours,
//! then validate the resulting `rtc_time` before returning it.

use spin::Mutex;
use x86_64::instructions::port::Port;

lazy_static::lazy_static! {
    static ref RTC_LOCK: Mutex<()> = Mutex::new(());
}

const CMOS_ADDR: u16 = 0x70;
const CMOS_DATA: u16 = 0x71;

const RTC_SECONDS: u8 = 0x00;
const RTC_MINUTES: u8 = 0x02;
const RTC_HOURS: u8 = 0x04;
const RTC_WEEKDAY: u8 = 0x06;
const RTC_DAY_OF_MONTH: u8 = 0x07;
const RTC_MONTH: u8 = 0x08;
const RTC_YEAR: u8 = 0x09;
const RTC_STATUS_A: u8 = 0x0A;
const RTC_STATUS_B: u8 = 0x0B;
const RTC_CENTURY: u8 = 0x32;

const RTC_UIP: u8 = 0x80;
const RTC_SET: u8 = 0x80;
const RTC_DM_BINARY: u8 = 0x04;
const RTC_24H: u8 = 0x02;

/// Standard Linux rtc_time structure
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtcTime {
    pub sec: i32,
    pub min: i32,
    pub hour: i32,
    pub mday: i32,
    pub mon: i32,  // 0-11
    pub year: i32, // Years since 1900
    pub wday: i32,
    pub yday: i32,
    pub isdst: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct RawRtcTime {
    sec: u8,
    min: u8,
    hour: u8,
    wday: u8,
    mday: u8,
    mon: u8,
    year: u8,
    century: u8,
    status_b: u8,
}

/// Convert BCD to binary after validating both nibbles.
fn bcd_to_binary_checked(bcd: u8) -> Option<u8> {
    let low = bcd & 0x0F;
    let high = (bcd >> 4) & 0x0F;
    if low > 9 || high > 9 {
        None
    } else {
        Some(low + high * 10)
    }
}

/// Convert binary to BCD.
fn binary_to_bcd(bin: u8) -> u8 {
    ((bin / 10) << 4) | (bin % 10)
}

fn cmos_read(reg: u8) -> u8 {
    unsafe {
        let mut addr = Port::<u8>::new(CMOS_ADDR);
        let mut data = Port::<u8>::new(CMOS_DATA);
        addr.write(reg);
        data.read()
    }
}

fn cmos_write(reg: u8, value: u8) {
    unsafe {
        let mut addr = Port::<u8>::new(CMOS_ADDR);
        let mut data = Port::<u8>::new(CMOS_DATA);
        addr.write(reg);
        data.write(value);
    }
}

/// Check if RTC is currently updating.
fn is_updating() -> bool {
    (cmos_read(RTC_STATUS_A) & RTC_UIP) != 0
}

fn wait_for_update_complete() -> Result<(), &'static str> {
    for _ in 0..100_000 {
        if !is_updating() {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err("rtc: update-in-progress did not clear")
}

fn read_raw_time() -> Result<RawRtcTime, &'static str> {
    wait_for_update_complete()?;
    Ok(RawRtcTime {
        sec: cmos_read(RTC_SECONDS),
        min: cmos_read(RTC_MINUTES),
        hour: cmos_read(RTC_HOURS),
        wday: cmos_read(RTC_WEEKDAY),
        mday: cmos_read(RTC_DAY_OF_MONTH),
        mon: cmos_read(RTC_MONTH),
        year: cmos_read(RTC_YEAR),
        century: cmos_read(RTC_CENTURY),
        status_b: cmos_read(RTC_STATUS_B),
    })
}

fn decode_field(raw: u8, binary_mode: bool) -> Result<u8, &'static str> {
    if binary_mode {
        Ok(raw)
    } else {
        bcd_to_binary_checked(raw).ok_or("rtc: invalid BCD value")
    }
}

fn decode_hour(raw_hour: u8, status_b: u8) -> Result<u8, &'static str> {
    let binary_mode = (status_b & RTC_DM_BINARY) != 0;
    let is_24h = (status_b & RTC_24H) != 0;
    let pm = !is_24h && (raw_hour & 0x80) != 0;
    let hour_raw = if is_24h { raw_hour } else { raw_hour & 0x7F };
    let mut hour = decode_field(hour_raw, binary_mode)?;

    if !is_24h {
        if hour == 0 || hour > 12 {
            return Err("rtc: invalid 12-hour clock value");
        }
        if hour == 12 {
            hour = 0;
        }
        if pm {
            hour += 12;
        }
    }

    Ok(hour)
}

fn days_in_month(mon: i32, full_year: i32) -> i32 {
    match mon {
        0 | 2 | 4 | 6 | 7 | 9 | 11 => 31,
        3 | 5 | 8 | 10 => 30,
        1 if is_leap_year(full_year) => 29,
        1 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn calculate_yday(mon: i32, mday: i32, full_year: i32) -> i32 {
    let mut yday = mday - 1;
    let mut m = 0;
    while m < mon {
        yday += days_in_month(m, full_year);
        m += 1;
    }
    yday
}

/// Sakamoto's algorithm, returning Linux `rtc_time` weekday (Sunday = 0).
fn calculate_wday(full_year: i32, mon: i32, mday: i32) -> i32 {
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut year = full_year;
    let month = mon as usize;
    if month < 2 {
        year -= 1;
    }
    (year + year / 4 - year / 100 + year / 400 + OFFSETS[month] + mday) % 7
}

fn validate_time(time: &RtcTime) -> Result<(), &'static str> {
    if !(0..=59).contains(&time.sec) {
        return Err("rtc: seconds out of range");
    }
    if !(0..=59).contains(&time.min) {
        return Err("rtc: minutes out of range");
    }
    if !(0..=23).contains(&time.hour) {
        return Err("rtc: hour out of range");
    }
    if !(0..=11).contains(&time.mon) {
        return Err("rtc: month out of range");
    }
    let full_year = time.year + 1900;
    if full_year < 1970 || full_year > 2099 {
        return Err("rtc: year out of supported range");
    }
    let dim = days_in_month(time.mon, full_year);
    if time.mday < 1 || time.mday > dim {
        return Err("rtc: day-of-month out of range");
    }
    Ok(())
}

fn decode_raw_time(raw: RawRtcTime) -> Result<RtcTime, &'static str> {
    let binary_mode = (raw.status_b & RTC_DM_BINARY) != 0;

    let sec = decode_field(raw.sec, binary_mode)? as i32;
    let min = decode_field(raw.min, binary_mode)? as i32;
    let hour = decode_hour(raw.hour, raw.status_b)? as i32;
    let mday = decode_field(raw.mday, binary_mode)? as i32;
    let mon = decode_field(raw.mon, binary_mode)? as i32 - 1;
    let year_short = decode_field(raw.year, binary_mode)? as i32;

    let full_year = match raw.century {
        0x00 | 0xFF => {
            if year_short >= 70 {
                1900 + year_short
            } else {
                2000 + year_short
            }
        }
        century_raw => {
            let century = decode_field(century_raw, binary_mode)? as i32;
            century * 100 + year_short
        }
    };

    let mut time = RtcTime {
        sec,
        min,
        hour,
        mday,
        mon,
        year: full_year - 1900,
        wday: 0,
        yday: 0,
        isdst: -1,
    };
    validate_time(&time)?;

    let raw_wday = decode_field(raw.wday, binary_mode).unwrap_or(0) as i32;
    time.wday = if (1..=7).contains(&raw_wday) {
        raw_wday % 7
    } else {
        calculate_wday(full_year, mon, mday)
    };
    time.yday = calculate_yday(mon, mday, full_year);

    Ok(time)
}

/// Read the current time from the RTC CMOS registers.
pub fn read_time() -> Result<RtcTime, &'static str> {
    let _lock = RTC_LOCK.lock();

    // The MC146818 can update while software is reading.  Sample twice and only
    // accept matching values, equivalent to Linux's stable CMOS read path.
    let mut previous = read_raw_time()?;
    for _ in 0..8 {
        let current = read_raw_time()?;
        if current == previous {
            return decode_raw_time(current);
        }
        previous = current;
    }

    Err("rtc: CMOS time did not stabilize")
}

fn encode_field(value: u8, binary_mode: bool) -> u8 {
    if binary_mode {
        value
    } else {
        binary_to_bcd(value)
    }
}

fn encode_hour(hour_24: u8, status_b: u8) -> u8 {
    let binary_mode = (status_b & RTC_DM_BINARY) != 0;
    if (status_b & RTC_24H) != 0 {
        return encode_field(hour_24, binary_mode);
    }

    let pm = hour_24 >= 12;
    let mut hour_12 = hour_24 % 12;
    if hour_12 == 0 {
        hour_12 = 12;
    }
    let mut encoded = encode_field(hour_12, binary_mode);
    if pm {
        encoded |= 0x80;
    }
    encoded
}

/// Write a new time to the RTC CMOS registers.
pub fn write_time(time: &RtcTime) -> Result<(), &'static str> {
    validate_time(time)?;
    let _lock = RTC_LOCK.lock();

    wait_for_update_complete()?;

    let status_b = cmos_read(RTC_STATUS_B);
    let binary_mode = (status_b & RTC_DM_BINARY) != 0;
    let full_year = time.year + 1900;
    let year_short = (full_year % 100) as u8;
    let century = (full_year / 100) as u8;

    // Freeze updates while programming the split CMOS fields, then restore the
    // original Status B mode bits exactly as firmware configured them.
    cmos_write(RTC_STATUS_B, status_b | RTC_SET);

    cmos_write(RTC_SECONDS, encode_field(time.sec as u8, binary_mode));
    cmos_write(RTC_MINUTES, encode_field(time.min as u8, binary_mode));
    cmos_write(RTC_HOURS, encode_hour(time.hour as u8, status_b));
    cmos_write(
        RTC_WEEKDAY,
        encode_field(
            (calculate_wday(full_year, time.mon, time.mday) + 1) as u8,
            binary_mode,
        ),
    );
    cmos_write(RTC_DAY_OF_MONTH, encode_field(time.mday as u8, binary_mode));
    cmos_write(RTC_MONTH, encode_field((time.mon + 1) as u8, binary_mode));
    cmos_write(RTC_YEAR, encode_field(year_short, binary_mode));
    cmos_write(RTC_CENTURY, encode_field(century, binary_mode));

    cmos_write(RTC_STATUS_B, status_b & !RTC_SET);
    Ok(())
}
