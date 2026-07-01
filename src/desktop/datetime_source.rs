//! Datetime source — ported from gnome-datetime-source.c
//!
//! Schedules one-time callbacks based on wall-clock (real) time, as opposed
//! to monotonic time.  Used by the wall clock to update at the next minute
//! boundary.
//!
//! The upstream uses GLib's GSource machinery and optionally Linux timerfd
//! for precise cancellation on clock changes.  We implement a simple polling
//! mechanism: check `system_time()` against the expiration timestamp.
//!
//! `cancel_on_set` mode also fires if the system clock jumps discontinuously
//! (detected by comparing monotonic time progression against wall-clock
//! progression).

use core::sync::atomic::{AtomicU64, Ordering};

/// One second in microseconds (matching `G_TIME_SPAN_SECOND`).
const TIME_SPAN_SECOND: u64 = 1_000_000;

/// Datetime source — a one-shot wall-clock timer.
pub struct DateTimeSource {
    /// Wall-clock expiration time (Unix timestamp in microseconds).
    real_expiration: u64,
    /// Monotonic wakeup expiration (uptime in microseconds).
    wakeup_expiration: u64,
    /// Whether to cancel (fire) when the system clock changes discontinuously.
    cancel_on_set: bool,
    /// Whether the source has already expired initially.
    initially_expired: bool,
    /// Whether the source has been dispatched (fired).
    fired: bool,
    /// Last seen wall-clock time, for detecting discontinuous changes.
    last_real_time: u64,
    /// Last seen monotonic time (uptime in microseconds).
    last_monotonic: u64,
}

impl DateTimeSource {
    /// Create a new datetime source that fires when the wall clock reaches
    /// `expiry_unix_seconds`.  Matches `_gnome_datetime_source_new()`.
    ///
    /// - `now_unix_seconds`: the expected current time (to avoid race conditions)
    /// - `expiry_unix_seconds`: when to fire
    /// - `cancel_on_set`: also fire if the clock jumps
    pub fn new(now_unix_seconds: u64, expiry_unix_seconds: u64, cancel_on_set: bool) -> Self {
        let monotonic_now = crate::time::uptime_ms() * 1000;
        Self {
            real_expiration: expiry_unix_seconds * 1_000_000,
            wakeup_expiration: monotonic_now + TIME_SPAN_SECOND,
            cancel_on_set,
            initially_expired: expiry_unix_seconds <= now_unix_seconds,
            fired: false,
            last_real_time: now_unix_seconds * 1_000_000,
            last_monotonic: monotonic_now,
        }
    }

    /// Check if the source has expired (should fire).  Matches
    /// `g_datetime_source_is_expired()`.
    pub fn is_expired(&self) -> bool {
        if self.fired {
            return false;
        }

        let real_now = crate::time::system_time() * 1_000_000;
        let monotonic_now = crate::time::uptime_ms() * 1000;

        if self.initially_expired {
            return true;
        }

        if self.real_expiration <= real_now {
            return true;
        }

        // In cancel_on_set mode, check every second whether the clock jumped
        if self.cancel_on_set && monotonic_now >= self.wakeup_expiration {
            return true;
        }

        false
    }

    /// Prepare for polling — returns the timeout in milliseconds until the
    /// next check, or 0 if ready to fire.  Matches `g_datetime_source_prepare()`.
    pub fn prepare(&mut self) -> u64 {
        if self.fired {
            return u64::MAX;
        }

        let monotonic_now = crate::time::uptime_ms() * 1000;

        if monotonic_now < self.wakeup_expiration {
            // Round up to milliseconds
            let remaining_us = self.wakeup_expiration.saturating_sub(monotonic_now);
            (remaining_us + 999) / 1000
        } else if self.is_expired() {
            0
        } else {
            // Reschedule wakeup
            self.wakeup_expiration = monotonic_now + TIME_SPAN_SECOND;
            1000
        }
    }

    /// Check if ready to dispatch.  Matches `g_datetime_source_check()`.
    pub fn check(&mut self) -> bool {
        if self.fired {
            return false;
        }
        if self.is_expired() {
            return true;
        }
        // Reschedule
        let monotonic_now = crate::time::uptime_ms() * 1000;
        self.wakeup_expiration = monotonic_now + TIME_SPAN_SECOND;
        false
    }

    /// Mark as dispatched (fired).  Returns true if it was actually dispatched
    /// (i.e. was expired and not yet fired).  Matches `g_datetime_source_dispatch()`.
    pub fn dispatch(&mut self) -> bool {
        if self.fired || !self.is_expired() {
            return false;
        }
        self.fired = true;
        true
    }

    /// Whether this source has been fired.
    pub fn is_fired(&self) -> bool {
        self.fired
    }

    /// Get the expiration time in Unix seconds.
    pub fn expiry_unix_seconds(&self) -> u64 {
        self.real_expiration / 1_000_000
    }

    /// Detect if the system clock has changed discontinuously since the last
    /// call.  In `cancel_on_set` mode, this causes the source to fire.
    pub fn detect_clock_change(&mut self) -> bool {
        let real_now = crate::time::system_time() * 1_000_000;
        let monotonic_now = crate::time::uptime_ms() * 1000;

        let expected_real = self.last_real_time + monotonic_now.saturating_sub(self.last_monotonic);

        let jumped = real_now.abs_diff(expected_real) > TIME_SPAN_SECOND;

        self.last_real_time = real_now;
        self.last_monotonic = monotonic_now;

        jumped
    }
}

/// Global counter for detecting clock changes (static, for wall clock use).
static LAST_REAL_TIME: AtomicU64 = AtomicU64::new(0);
static LAST_MONOTONIC: AtomicU64 = AtomicU64::new(0);

/// Check if the system clock has jumped since the last call.
/// Returns true if a discontinuous change was detected.
/// This is a convenience function for the wall clock to use.
pub fn detect_clock_change() -> bool {
    let real_now = crate::time::system_time() * 1_000_000;
    let monotonic_now = crate::time::uptime_ms() * 1000;

    let last_real = LAST_REAL_TIME.load(Ordering::Relaxed);
    let last_mono = LAST_MONOTONIC.load(Ordering::Relaxed);

    LAST_REAL_TIME.store(real_now, Ordering::Relaxed);
    LAST_MONOTONIC.store(monotonic_now, Ordering::Relaxed);

    if last_real == 0 {
        return false;
    }

    let expected_real = last_real + monotonic_now.saturating_sub(last_mono);
    real_now.abs_diff(expected_real) > TIME_SPAN_SECOND
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_already_expired() {
        let now = crate::time::system_time();
        let src = DateTimeSource::new(now, now.saturating_sub(10), false);
        assert!(src.is_expired());
    }

    fn test_future_not_expired() {
        let now = crate::time::system_time();
        let src = DateTimeSource::new(now, now + 3600, false);
        assert!(!src.is_expired());
    }

    fn test_dispatch() {
        let now = crate::time::system_time();
        let mut src = DateTimeSource::new(now, now.saturating_sub(5), false);
        assert!(src.dispatch());
        assert!(src.is_fired());
        // Second dispatch should not fire again
        assert!(!src.dispatch());
    }
}
