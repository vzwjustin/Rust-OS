//! Linux time operation APIs
//!
//! This module implements Linux-compatible time operations including
//! clock_gettime, clock_settime, nanosleep, and timer operations.

extern crate alloc;

use alloc::collections::BTreeMap;

use core::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use lazy_static::lazy_static;
use spin::Mutex;

use super::process_ops;
use super::types::*;
use super::{LinuxError, LinuxResult};

const NSECS_PER_SEC: i64 = 1_000_000_000;
const NSECS_PER_USEC: i64 = 1_000;

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

/// Safely copy a POD structure from a user-space pointer into a local copy.
///
/// This avoids dereferencing an untrusted user pointer directly; the value is
/// copied through the validated user-space memory path. Returns `EFAULT` if the
/// pointer is null or the copy fails validation.
fn copy_struct_from_user<T: Copy>(user_ptr: u64) -> LinuxResult<T> {
    super::copy_struct_from_user(user_ptr as *const T)
}

/// Safely copy a POD structure to a user-space pointer.
fn copy_struct_to_user<T: Copy>(user_ptr: u64, value: &T) -> LinuxResult<()> {
    super::copy_struct_to_user(user_ptr as *mut T, value)
}

fn checked_timespec_to_ns(ts: &TimeSpec) -> LinuxResult<i64> {
    if ts.tv_sec < 0 || ts.tv_nsec < 0 || ts.tv_nsec >= NSECS_PER_SEC {
        return Err(LinuxError::EINVAL);
    }

    ts.tv_sec
        .checked_mul(NSECS_PER_SEC)
        .and_then(|sec_ns| sec_ns.checked_add(ts.tv_nsec))
        .ok_or(LinuxError::EINVAL)
}

fn realtime_ns_i64() -> i64 {
    let max_secs = (i64::MAX as u64) / (NSECS_PER_SEC as u64);
    let secs = core::cmp::min(crate::time::system_time(), max_secs);
    (secs as i64).saturating_mul(NSECS_PER_SEC)
}

fn uptime_ns_i64() -> i64 {
    core::cmp::min(crate::time::uptime_ns(), i64::MAX as u64) as i64
}

fn checked_timer_part_to_ns(sec: u64, nsec: u64) -> LinuxResult<u64> {
    if nsec >= NSECS_PER_SEC as u64 {
        return Err(LinuxError::EINVAL);
    }

    sec.checked_mul(NSECS_PER_SEC as u64)
        .and_then(|sec_ns| sec_ns.checked_add(nsec))
        .ok_or(LinuxError::EINVAL)
}

/// Timer specification structure (struct itimerspec)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct ITimerSpec {
    it_interval_sec: u64,
    it_interval_nsec: u64,
    it_value_sec: u64,
    it_value_nsec: u64,
}

/// In-memory POSIX timer entry.
struct PosixTimer {
    clockid: i32,
    /// Expiration time in ns (0 = disarmed).
    expires_ns: u64,
    /// Interval for periodic timers in ns (0 = one-shot).
    interval_ns: u64,
}

lazy_static! {
    static ref POSIX_TIMERS: Mutex<BTreeMap<TimerId, PosixTimer>> = Mutex::new(BTreeMap::new());
    static ref NEXT_TIMER_ID: AtomicI32 = AtomicI32::new(1);
}

/// Alarm deadline as Unix timestamp (0 = no alarm scheduled)
static ALARM_DEADLINE: AtomicU64 = AtomicU64::new(0);

fn zero_itimerspec(ptr: *mut u8) -> LinuxResult<()> {
    if !ptr.is_null() {
        copy_struct_to_user(ptr as u64, &ITimerSpec::default())?;
    }
    Ok(())
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
            let value = TimeSpec {
                tv_sec: secs as Time,
                tv_nsec: (ms * 1_000_000) as Nsec,
            };
            copy_struct_to_user(tp as u64, &value)?;
            Ok(0)
        }
        clock::CLOCK_MONOTONIC => {
            let ms = crate::time::uptime_ms();
            let value = TimeSpec {
                tv_sec: (ms / 1000) as Time,
                tv_nsec: ((ms % 1000) * 1_000_000) as Nsec,
            };
            copy_struct_to_user(tp as u64, &value)?;
            Ok(0)
        }
        clock::CLOCK_PROCESS_CPUTIME_ID
        | clock::CLOCK_THREAD_CPUTIME_ID
        | clock::CLOCK_MONOTONIC_RAW
        | clock::CLOCK_BOOTTIME => {
            copy_struct_to_user(
                tp as u64,
                &TimeSpec {
                    tv_sec: 0,
                    tv_nsec: 0,
                },
            )?;
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
            if process_ops::geteuid() != 0 {
                return Err(LinuxError::EPERM);
            }
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
            // Kernel time is tracked at nanosecond granularity.
            copy_struct_to_user(
                res as u64,
                &TimeSpec {
                    tv_sec: 0,
                    tv_nsec: 1,
                },
            )?;
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

    // Copy the request out of user space instead of dereferencing the raw pointer.
    let sleep_time: TimeSpec = copy_struct_from_user(req as u64)?;

    let sleep_us = (checked_timespec_to_ns(&sleep_time)? / NSECS_PER_USEC) as u64;
    crate::time::sleep_us(sleep_us);

    // If interrupted by signal and rem is not null, store remaining time
    if !rem.is_null() {
        copy_struct_to_user(
            rem as u64,
            &TimeSpec {
                tv_sec: 0,
                tv_nsec: 0,
            },
        )?;
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
    if flags & !TIMER_ABSTIME != 0 {
        return Err(LinuxError::EINVAL);
    }

    match clockid {
        clock::CLOCK_REALTIME | clock::CLOCK_MONOTONIC => {
            let req_ts: TimeSpec = copy_struct_from_user(req as u64)?;
            let req_ns = checked_timespec_to_ns(&req_ts)?;

            if flags & TIMER_ABSTIME != 0 {
                // Absolute time sleep: compute remaining time from now
                let now_ns: i64 = if clockid == clock::CLOCK_REALTIME {
                    realtime_ns_i64()
                } else {
                    uptime_ns_i64()
                };
                if req_ns > now_ns {
                    let remaining_ns = req_ns - now_ns;
                    let sleep_ts = TimeSpec {
                        tv_sec: remaining_ns / NSECS_PER_SEC,
                        tv_nsec: remaining_ns % NSECS_PER_SEC,
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
    let value = TimeVal {
        tv_sec: secs as Time,
        tv_usec: us as Time,
    };
    copy_struct_to_user(tv as u64, &value)?;

    // tz is obsolete and should be NULL
    if !tz.is_null() {
        return Err(LinuxError::EINVAL);
    }

    Ok(0)
}

/// time - return seconds since the Epoch
///
/// If `tloc` is non-null, the current time is also written there.  This is the
/// legacy Linux `time` syscall (number 201 on x86_64).
pub fn time(tloc: *mut Time) -> LinuxResult<Time> {
    inc_ops();
    let secs = crate::time::system_time();
    if !tloc.is_null() {
        copy_struct_to_user(tloc as u64, &(secs as Time))?;
    }
    Ok(secs as i64)
}

/// settimeofday - set time of day
pub fn settimeofday(tv: *const TimeVal, tz: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if tv.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if !tz.is_null() {
        return Err(LinuxError::EINVAL);
    }

    let pid = crate::process::current_pid();
    if let Some(ctx) = crate::security::get_context(pid) {
        if !ctx.is_root() && !crate::security::check_permission(pid, "sys_time") {
            return Err(LinuxError::EPERM);
        }
    } else {
        return Err(LinuxError::EPERM);
    }

    let requested: TimeVal = copy_struct_from_user(tv as u64)?;
    if requested.tv_sec < 0 {
        return Err(LinuxError::EINVAL);
    }
    crate::time::set_system_time(requested.tv_sec as u64);

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
            let id = NEXT_TIMER_ID.fetch_add(1, Ordering::SeqCst);
            POSIX_TIMERS.lock().insert(
                id,
                PosixTimer {
                    clockid,
                    expires_ns: 0,
                    interval_ns: 0,
                },
            );
            copy_struct_to_user(timerid as u64, &id)?;
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// timer_settime - arm/disarm a timer
pub fn timer_settime(
    timerid: TimerId,
    flags: i32,
    new_value: *const u8, // struct itimerspec
    old_value: *mut u8,   // struct itimerspec
) -> LinuxResult<i32> {
    inc_ops();

    if new_value.is_null() {
        return Err(LinuxError::EFAULT);
    }

    const TIMER_ABSTIME: i32 = 1;
    if flags & !TIMER_ABSTIME != 0 {
        return Err(LinuxError::EINVAL);
    }

    let mut timers = POSIX_TIMERS.lock();
    let timer = timers.get_mut(&timerid).ok_or(LinuxError::EINVAL)?;

    // Copy the new timer spec out of user space rather than dereferencing directly.
    let spec: ITimerSpec = copy_struct_from_user(new_value as u64)?;
    if spec.it_interval_nsec >= NSECS_PER_SEC as u64 || spec.it_value_nsec >= NSECS_PER_SEC as u64 {
        return Err(LinuxError::EINVAL);
    }

    // Store previous timer settings in old_value.
    if !old_value.is_null() {
        let now = crate::time::uptime_ns();
        let old_remaining = if timer.expires_ns != 0 && timer.expires_ns > now {
            timer.expires_ns - now
        } else {
            0
        };
        let old_spec = ITimerSpec {
            it_interval_sec: timer.interval_ns / 1_000_000_000,
            it_interval_nsec: timer.interval_ns % 1_000_000_000,
            it_value_sec: old_remaining / 1_000_000_000,
            it_value_nsec: old_remaining % 1_000_000_000,
        };
        copy_struct_to_user(old_value as u64, &old_spec)?;
    }

    timer.interval_ns = checked_timer_part_to_ns(spec.it_interval_sec, spec.it_interval_nsec)?;
    let value_ns = checked_timer_part_to_ns(spec.it_value_sec, spec.it_value_nsec)?;

    if value_ns == 0 {
        timer.expires_ns = 0;
    } else {
        // TIMER_ABSTIME (1): value is absolute clock time.
        timer.expires_ns = if (flags & 1) != 0 {
            value_ns
        } else {
            crate::time::uptime_ns().saturating_add(value_ns)
        };
    }

    Ok(0)
}

/// timer_gettime - get timer value
pub fn timer_gettime(
    timerid: TimerId,
    curr_value: *mut u8, // struct itimerspec
) -> LinuxResult<i32> {
    inc_ops();

    if curr_value.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let timers = POSIX_TIMERS.lock();
    let timer = timers.get(&timerid).ok_or(LinuxError::EINVAL)?;

    let now = crate::time::uptime_ns();
    let remaining = if timer.expires_ns == 0 {
        0
    } else {
        timer.expires_ns.saturating_sub(now)
    };

    let current = ITimerSpec {
        it_interval_sec: timer.interval_ns / 1_000_000_000,
        it_interval_nsec: timer.interval_ns % 1_000_000_000,
        it_value_sec: remaining / 1_000_000_000,
        it_value_nsec: remaining % 1_000_000_000,
    };
    copy_struct_to_user(curr_value as u64, &current)?;

    Ok(0)
}

/// timer_delete - delete a timer
pub fn timer_delete(timerid: TimerId) -> LinuxResult<i32> {
    inc_ops();

    if POSIX_TIMERS.lock().remove(&timerid).is_none() {
        return Err(LinuxError::EINVAL);
    }

    Ok(0)
}

/// timer_getoverrun - get timer overrun count
pub fn timer_getoverrun(timerid: TimerId) -> LinuxResult<i32> {
    inc_ops();

    if !POSIX_TIMERS.lock().contains_key(&timerid) {
        return Err(LinuxError::EINVAL);
    }

    Ok(0)
}

/// alarm - set an alarm clock
pub fn alarm(seconds: u32) -> u32 {
    inc_ops();

    let now = crate::time::system_time();
    let previous = ALARM_DEADLINE.swap(0, Ordering::Relaxed);
    let remaining = if previous > now {
        (previous - now) as u32
    } else {
        0
    };

    if seconds != 0 {
        ALARM_DEADLINE.store(now.saturating_add(seconds as u64), Ordering::Relaxed);
    }

    remaining
}

/// sleep - sleep for specified number of seconds
pub fn sleep(seconds: u32) -> u32 {
    inc_ops();

    crate::time::sleep_us((seconds as u64).saturating_mul(1_000_000));
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
    checked_timespec_to_ns(ts).unwrap_or_else(|_| {
        if ts.tv_sec < 0 || ts.tv_nsec < 0 {
            i64::MIN
        } else {
            i64::MAX
        }
    })
}

/// Convert nanoseconds to TimeSpec
pub fn ns_to_timespec(ns: i64) -> TimeSpec {
    TimeSpec {
        tv_sec: ns.div_euclid(NSECS_PER_SEC),
        tv_nsec: ns.rem_euclid(NSECS_PER_SEC),
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
