//! Watchdog timer driver for RustOS
//!
//! Provides a software-based watchdog timer that will panic/reset the system
//! if it is not "kicked" (written to) within a configurable timeout period.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static WATCHDOG_TIMEOUT: AtomicU32 = AtomicU32::new(10); // Default 10 seconds
static WATCHDOG_REMAINING: AtomicU32 = AtomicU32::new(10);
static WATCHDOG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Incrementally count down the watchdog timer.
///
/// This should be called once per second (e.g., from the timer interrupt).
pub fn watchdog_tick() {
    if WATCHDOG_ENABLED.load(Ordering::Acquire) {
        let remaining = WATCHDOG_REMAINING.load(Ordering::Acquire);
        if remaining == 0 {
            // Watchdog expired! Panic the kernel.
            panic!("WATCHDOG TIMER EXPIRED: System lockup detected!");
        } else {
            WATCHDOG_REMAINING.store(remaining - 1, Ordering::Release);
        }
    }
}

/// Reset (kick) the watchdog timer back to its configured timeout.
pub fn kick() {
    let timeout = WATCHDOG_TIMEOUT.load(Ordering::Acquire);
    WATCHDOG_REMAINING.store(timeout, Ordering::Release);
}

/// Set the watchdog timeout in seconds.
pub fn set_timeout(seconds: u32) {
    WATCHDOG_TIMEOUT.store(seconds, Ordering::Release);
    // Automatically reset the remaining time if already running
    if WATCHDOG_ENABLED.load(Ordering::Acquire) {
        WATCHDOG_REMAINING.store(seconds, Ordering::Release);
    }
}

/// Get the current watchdog timeout in seconds.
pub fn get_timeout() -> u32 {
    WATCHDOG_TIMEOUT.load(Ordering::Acquire)
}

/// Get the remaining time before expiration in seconds.
pub fn get_timeleft() -> u32 {
    if WATCHDOG_ENABLED.load(Ordering::Acquire) {
        WATCHDOG_REMAINING.load(Ordering::Acquire)
    } else {
        0
    }
}

/// Enable the watchdog timer.
pub fn enable() {
    let timeout = WATCHDOG_TIMEOUT.load(Ordering::Acquire);
    WATCHDOG_REMAINING.store(timeout, Ordering::Release);
    WATCHDOG_ENABLED.store(true, Ordering::Release);
}

/// Disable the watchdog timer.
pub fn disable() {
    WATCHDOG_ENABLED.store(false, Ordering::Release);
}

/// Check if the watchdog timer is enabled.
pub fn is_enabled() -> bool {
    WATCHDOG_ENABLED.load(Ordering::Acquire)
}
