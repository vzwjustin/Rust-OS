//! Linux time operation APIs
//!
//! This module implements Linux-compatible time operations including
//! clock_gettime, clock_settime, nanosleep, and timer operations.

use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxError, LinuxResult};

/// Operation counter for statistics
static TIME_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize time operations subsystem
pub fn init_time_operations() {
    TIME_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of time operations performed
pub fn get_operation_count() -> u64 {
    TIME_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    TIME_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// clock_gettime - get time of specified clock
pub fn clock_gettime(clockid: i32, tp: *mut TimeSpec) -> LinuxResult<i32> {
    inc_ops();

    if tp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    match clockid {
        clock::CLOCK_REALTIME => {
            let secs = crate::time::system_time();
            let ms = crate::time::uptime_ms() % 1000;
            unsafe {
                (*tp).tv_sec = secs as Time;
                (*tp).tv_nsec = (ms * 1_000_000) as Nsec;
            }
            Ok(0)
        }
        clock::CLOCK_MONOTONIC => {
            let ms = crate::time::uptime_ms();
            unsafe {
                (*tp).tv_sec = (ms / 1000) as Time;
                (*tp).tv_nsec = ((ms % 1000) * 1_000_000) as Nsec;
            }
            Ok(0)
        }
        clock::CLOCK_PROCESS_CPUTIME_ID
        | clock::CLOCK_THREAD_CPUTIME_ID
        | clock::CLOCK_MONOTONIC_RAW
        | clock::CLOCK_BOOTTIME => {
            unsafe {
                (*tp).tv_sec = 0;
                (*tp).tv_nsec = 0;
            }
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// clock_settime - set time of specified clock
pub fn clock_settime(clockid: i32, tp: *const TimeSpec) -> LinuxResult<i32> {
    inc_ops();

    if tp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    match clockid {
        clock::CLOCK_REALTIME => {
            // TODO: Set real-time clock (requires privileges)
            // Check if current process has CAP_SYS_TIME capability
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL), // Only CLOCK_REALTIME can be set
    }
}

/// clock_getres - get clock resolution
pub fn clock_getres(clockid: i32, res: *mut TimeSpec) -> LinuxResult<i32> {
    inc_ops();

    if res.is_null() {
        return Err(LinuxError::EFAULT);
    }

    match clockid {
        clock::CLOCK_REALTIME
        | clock::CLOCK_MONOTONIC
        | clock::CLOCK_PROCESS_CPUTIME_ID
        | clock::CLOCK_THREAD_CPUTIME_ID
        | clock::CLOCK_MONOTONIC_RAW
        | clock::CLOCK_BOOTTIME => {
            // TODO: Get actual clock resolution
            // Most systems have nanosecond resolution
            unsafe {
                (*res).tv_sec = 0;
                (*res).tv_nsec = 1; // 1 nanosecond resolution
            }
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// nanosleep - high-resolution sleep
pub fn nanosleep(req: *const TimeSpec, rem: *mut TimeSpec) -> LinuxResult<i32> {
    inc_ops();

    if req.is_null() {
        return Err(LinuxError::EFAULT);
    }

    unsafe {
        let sleep_time = *req;

        // Validate sleep time
        if sleep_time.tv_sec < 0 || sleep_time.tv_nsec < 0 || sleep_time.tv_nsec >= 1_000_000_000 {
            return Err(LinuxError::EINVAL);
        }

        let sleep_us = sleep_time.tv_sec as u64 * 1_000_000 + (sleep_time.tv_nsec as u64 / 1_000);
        crate::time::sleep_us(sleep_us);

        // If interrupted by signal and rem is not null, store remaining time
        if !rem.is_null() {
            (*rem).tv_sec = 0;
            (*rem).tv_nsec = 0;
        }
    }

    Ok(0)
}

/// clock_nanosleep - high-resolution sleep on specific clock
pub fn clock_nanosleep(
    clockid: i32,
    flags: i32,
    req: *const TimeSpec,
    rem: *mut TimeSpec,
) -> LinuxResult<i32> {
    inc_ops();

    if req.is_null() {
        return Err(LinuxError::EFAULT);
    }

    const TIMER_ABSTIME: i32 = 1;

    match clockid {
        clock::CLOCK_REALTIME | clock::CLOCK_MONOTONIC => {
            if flags & TIMER_ABSTIME != 0 {
                // Absolute time sleep: compute remaining time from now
                let now_ns: i64 = if clockid == clock::CLOCK_REALTIME {
                    crate::time::system_time() as i64 * 1_000_000_000
                } else {
                    crate::time::uptime_ns() as i64
                };
                let target_ns = unsafe { (*req).tv_sec as i64 * 1_000_000_000 + (*req).tv_nsec as i64 };
                if target_ns > now_ns {
                    let remaining_ns = target_ns - now_ns;
                    let sleep_ts = TimeSpec {
                        tv_sec: remaining_ns / 1_000_000_000,
                        tv_nsec: remaining_ns % 1_000_000_000,
                    };
                    return nanosleep(&sleep_ts, rem);
                }
                Ok(0)
            } else {
                nanosleep(req, rem)
            }
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// gettimeofday - get time of day
pub fn gettimeofday(tv: *mut TimeVal, tz: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if tv.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Get actual time of day from kernel time subsystem
    let secs = crate::time::system_time();
    let us = crate::time::uptime_us() % 1_000_000;
    unsafe {
        (*tv).tv_sec = secs as Time;
        (*tv).tv_usec = us as Time;
    }

    // tz is obsolete and should be NULL
    if !tz.is_null() {
        return Err(LinuxError::EINVAL);
    }

    Ok(0)
}

/// settimeofday - set time of day
pub fn settimeofday(tv: *const TimeVal, tz: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if tv.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Set time of day (requires privileges)
    // tz is obsolete
    if !tz.is_null() {
        return Err(LinuxError::EINVAL);
    }

    Ok(0)
}

/// Timer ID type
pub type TimerId = i32;

/// timer_create - create a POSIX timer
pub fn timer_create(
    clockid: i32,
    _sevp: *const u8, // struct sigevent
    timerid: *mut TimerId,
) -> LinuxResult<i32> {
    inc_ops();

    if timerid.is_null() {
        return Err(LinuxError::EFAULT);
    }

    match clockid {
        clock::CLOCK_REALTIME | clock::CLOCK_MONOTONIC => {
            // TODO: Create actual timer
            // For now, return a dummy timer ID
            unsafe {
                *timerid = 1;
            }
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// timer_settime - arm/disarm a timer
pub fn timer_settime(
    _timerid: TimerId,
    _flags: i32,
    new_value: *const u8, // struct itimerspec
    _old_value: *mut u8,   // struct itimerspec
) -> LinuxResult<i32> {
    inc_ops();

    if new_value.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Set timer value
    Ok(0)
}

/// timer_gettime - get timer value
pub fn timer_gettime(
    _timerid: TimerId,
    curr_value: *mut u8, // struct itimerspec
) -> LinuxResult<i32> {
    inc_ops();

    if curr_value.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Get timer value
    Ok(0)
}

/// timer_delete - delete a timer
pub fn timer_delete(_timerid: TimerId) -> LinuxResult<i32> {
    inc_ops();

    // TODO: Delete timer
    Ok(0)
}

/// timer_getoverrun - get timer overrun count
pub fn timer_getoverrun(_timerid: TimerId) -> LinuxResult<i32> {
    inc_ops();

    // TODO: Get overrun count
    Ok(0)
}

/// alarm - set an alarm clock
pub fn alarm(_seconds: u32) -> u32 {
    inc_ops();

    // TODO: Set alarm
    // Return seconds remaining on previous alarm, or 0
    0
}

/// sleep - sleep for specified number of seconds
pub fn sleep(seconds: u32) -> u32 {
    inc_ops();

    crate::time::sleep_us(seconds as u64 * 1_000_000);
    0
}

/// usleep - suspend execution for microsecond intervals
pub fn usleep(usec: u32) -> LinuxResult<i32> {
    inc_ops();

    if usec >= 1_000_000 {
        return Err(LinuxError::EINVAL);
    }

    crate::time::sleep_us(usec as u64);
    Ok(0)
}

/// Convert TimeSpec to nanoseconds
pub fn timespec_to_ns(ts: &TimeSpec) -> i64 {
    ts.tv_sec * 1_000_000_000 + ts.tv_nsec
}

/// Convert nanoseconds to TimeSpec
pub fn ns_to_timespec(ns: i64) -> TimeSpec {
    TimeSpec {
        tv_sec: ns / 1_000_000_000,
        tv_nsec: ns % 1_000_000_000,
    }
}

#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_clock_operations() {
        let mut ts = TimeSpec::zero();
        assert!(clock_gettime(clock::CLOCK_REALTIME, &mut ts).is_ok());

        let mut res = TimeSpec::zero();
        assert!(clock_getres(clock::CLOCK_REALTIME, &mut res).is_ok());
        assert_eq!(res.tv_nsec, 1);
    }

    #[test_case]
    fn test_timespec_conversion() {
        let ns = 1_234_567_890;
        let ts = ns_to_timespec(ns);
        assert_eq!(ts.tv_sec, 1);
        assert_eq!(ts.tv_nsec, 234_567_890);

        let converted_back = timespec_to_ns(&ts);
        assert_eq!(converted_back, ns);
    }

    #[test_case]
    fn test_nanosleep_validation() {
        let mut invalid_ts = TimeSpec::new(0, 2_000_000_000); // Invalid nsec
        assert!(nanosleep(&invalid_ts, core::ptr::null_mut()).is_err());
    }
}
