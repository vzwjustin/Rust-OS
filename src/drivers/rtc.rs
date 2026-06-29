//! Real-Time Clock (RTC) driver for RustOS
//!
//! Provides access to the Motorola 146818 compatible CMOS RTC.

use spin::Mutex;
use x86_64::instructions::port::Port;

lazy_static::lazy_static! {
    static ref RTC_LOCK: Mutex<()> = Mutex::new(());
}

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

/// Convert BCD to binary
fn bcd_to_binary(bcd: u8) -> u8 {
    (bcd & 0x0F) + ((bcd >> 4) * 10)
}

/// Convert binary to BCD
fn binary_to_bcd(bin: u8) -> u8 {
    ((bin / 10) << 4) | (bin % 10)
}

/// Check if RTC is currently updating
fn is_updating() -> bool {
    unsafe {
        let mut addr = Port::<u8>::new(0x70);
        let mut data = Port::<u8>::new(0x71);
        addr.write(0x0A);
        (data.read() & 0x80) != 0
    }
}

/// Read the current time from the RTC CMOS registers.
pub fn read_time() -> Result<RtcTime, &'static str> {
    let _lock = RTC_LOCK.lock();

    // Wait for any update in progress to complete
    for _ in 0..10000 {
        if !is_updating() {
            break;
        }
        core::hint::spin_loop();
    }

    unsafe {
        let mut addr = Port::<u8>::new(0x70);
        let mut data = Port::<u8>::new(0x71);

        addr.write(0x00);
        let sec = bcd_to_binary(data.read()) as i32;

        addr.write(0x02);
        let min = bcd_to_binary(data.read()) as i32;

        addr.write(0x04);
        let hour = bcd_to_binary(data.read()) as i32;

        addr.write(0x07);
        let mday = bcd_to_binary(data.read()) as i32;

        addr.write(0x08);
        let mon = (bcd_to_binary(data.read()) as i32) - 1; // 0-11 in struct rtc_time

        addr.write(0x09);
        let mut year = bcd_to_binary(data.read()) as i32;

        // Try to read century from register 0x32
        addr.write(0x32);
        let century_raw = data.read();
        let century = if century_raw != 0 {
            bcd_to_binary(century_raw) as i32
        } else {
            20 // Default to 20th/21st century
        };

        year += (century * 100) - 1900; // Years since 1900

        Ok(RtcTime {
            sec,
            min,
            hour,
            mday,
            mon,
            year,
            wday: 0,
            yday: 0,
            isdst: -1,
        })
    }
}

/// Write a new time to the RTC CMOS registers.
pub fn write_time(time: &RtcTime) -> Result<(), &'static str> {
    let _lock = RTC_LOCK.lock();

    // Wait for any update in progress to complete
    for _ in 0..10000 {
        if !is_updating() {
            break;
        }
        core::hint::spin_loop();
    }

    unsafe {
        let mut addr = Port::<u8>::new(0x70);
        let mut data = Port::<u8>::new(0x71);

        // Disable updates during write
        addr.write(0x0B);
        let status_b = data.read();
        addr.write(0x0B);
        data.write(status_b | 0x80); // Set bit 7 to disable updates

        addr.write(0x00);
        data.write(binary_to_bcd(time.sec as u8));

        addr.write(0x02);
        data.write(binary_to_bcd(time.min as u8));

        addr.write(0x04);
        data.write(binary_to_bcd(time.hour as u8));

        addr.write(0x07);
        data.write(binary_to_bcd(time.mday as u8));

        addr.write(0x08);
        data.write(binary_to_bcd((time.mon + 1) as u8));

        let full_year = time.year + 1900;
        let year_short = (full_year % 100) as u8;
        let century = (full_year / 100) as u8;

        addr.write(0x09);
        data.write(binary_to_bcd(year_short));

        addr.write(0x32);
        data.write(binary_to_bcd(century));

        // Re-enable updates
        addr.write(0x0B);
        data.write(status_b & !0x80);
    }

    Ok(())
}
