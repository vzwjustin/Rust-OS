//! Timer matching `gtimer.h` / `gtimer.c`.
//!
//! A simple stopwatch timer. Uses a platform-provided clock function for
//! elapsed time. The clock source must be injected via `set_clock`.
//! Fully `no_std` compatible using `alloc` and `spin`.

use spin::Mutex;

/// A clock function that returns the current time in microseconds.
pub type ClockFn = fn() -> i64;

static CLOCK: Mutex<Option<ClockFn>> = Mutex::new(None);

/// Set the platform clock function.
///
/// Must be called before using `Timer`. The function should return
/// monotonic microseconds since some epoch.
pub fn set_clock(clock: ClockFn) {
    *CLOCK.lock() = Some(clock);
}

fn now() -> i64 {
    CLOCK.lock().map_or(0, |f| f())
}

/// Returns the current monotonic time in microseconds (`g_get_monotonic_time`).
///
/// Returns `0` if [`set_clock`] has not been called yet.
pub fn monotonic_time_us() -> i64 {
    now()
}

/// A timer (`GTimer`).
///
/// Measures elapsed time. Start/stop/continue/reset like a stopwatch.
pub struct Timer {
    start: i64,
    end: Option<i64>,
    active: bool,
}

impl Timer {
    /// Create a new timer, started (`g_timer_new`).
    pub fn new() -> Self {
        Self {
            start: now(),
            end: None,
            active: true,
        }
    }

    /// Start the timer (`g_timer_start`).
    pub fn start(&mut self) {
        self.start = now();
        self.end = None;
        self.active = true;
    }

    /// Stop the timer (`g_timer_stop`).
    pub fn stop(&mut self) {
        if self.active {
            self.end = Some(now());
            self.active = false;
        }
    }

    /// Reset the timer (`g_timer_reset`).
    pub fn reset(&mut self) {
        self.start = now();
        self.end = None;
        self.active = true;
    }

    /// Continue a stopped timer (`g_timer_continue`).
    pub fn continue_(&mut self) {
        if !self.active {
            if let Some(end_time) = self.end {
                let elapsed_before = end_time - self.start;
                self.start = now() - elapsed_before;
            }
            self.end = None;
            self.active = true;
        }
    }

    /// Get elapsed time (`g_timer_elapsed`).
    ///
    /// Returns (seconds, microseconds).
    pub fn elapsed(&self) -> (f64, u32) {
        let end = self.end.unwrap_or_else(now);
        let total_us = end - self.start;
        let secs = total_us as f64 / 1_000_000.0;
        let us = (total_us % 1_000_000) as u32;
        (secs, us)
    }

    /// Check if the timer is active (`g_timer_is_active`).
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_clock_0() -> i64 {
        0
    }
    fn mock_clock_1s() -> i64 {
        1_000_000
    }
    fn mock_clock_2_5s() -> i64 {
        2_500_000
    }

    #[test]
    fn timer_basic() {
        set_clock(mock_clock_0);
        let mut t = Timer::new();
        assert!(t.is_active());
        set_clock(mock_clock_1s);
        let (secs, us) = t.elapsed();
        assert!((secs - 1.0).abs() < 0.001);
        assert_eq!(us, 0);
    }

    #[test]
    fn timer_stop() {
        set_clock(mock_clock_0);
        let mut t = Timer::new();
        set_clock(mock_clock_1s);
        t.stop();
        assert!(!t.is_active());
        let (secs, _) = t.elapsed();
        assert!((secs - 1.0).abs() < 0.001);
        // Elapsed should not change after stop
        set_clock(mock_clock_2_5s);
        let (secs2, _) = t.elapsed();
        assert!((secs2 - 1.0).abs() < 0.001);
    }

    #[test]
    fn timer_continue() {
        set_clock(mock_clock_0);
        let mut t = Timer::new();
        set_clock(mock_clock_1s);
        t.stop();
        set_clock(mock_clock_2_5s);
        t.continue_();
        assert!(t.is_active());
        let (secs, _) = t.elapsed();
        assert!((secs - 1.0).abs() < 0.001);
    }

    #[test]
    fn timer_reset() {
        set_clock(mock_clock_0);
        let mut t = Timer::new();
        set_clock(mock_clock_1s);
        t.reset();
        let (secs, _) = t.elapsed();
        assert!((secs - 0.0).abs() < 0.001);
    }

    #[test]
    fn timer_microseconds() {
        set_clock(mock_clock_0);
        let t = Timer::new();
        set_clock(mock_clock_2_5s);
        let (secs, us) = t.elapsed();
        assert!((secs - 2.5).abs() < 0.001);
        assert_eq!(us, 500_000);
    }
}
