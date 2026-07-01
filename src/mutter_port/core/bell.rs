//! Visual bell logic ported from GNOME Mutter's src/core/bell.c
//!
//! Implements photosensitivity-safe visual bell notifications that decide
//! whether to flash a focused window or the entire screen in response to
//! bell events (e.g., from system alerts, terminal beep).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/bell.c

use crate::desktop::window_manager::WindowId;
use core::sync::atomic::{AtomicU64, Ordering};

/// Minimum milliseconds between visual bell alerts to prevent photosensitive seizures.
/// Enforces a max flash rate of 2Hz per WCAG/European Accessibility Act.
const MIN_TIME_BETWEEN_VISUAL_ALERTS_MS: u64 = 500;

/// Minimum milliseconds between double-flash alerts.
/// Alerts within 3 seconds of the last one use single flash; older ones use double flash.
const MIN_TIME_BETWEEN_DOUBLE_VISUAL_ALERT_MS: u64 = 3000;

/// Monotonic clock time (ms) of the last visual bell notification.
/// Shared mutable state protected by atomic ordering.
static LAST_VISUAL_BELL_TIME_MS: AtomicU64 = AtomicU64::new(0);

/// Result of a bell trigger: which region to flash or nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BellAction {
    /// Flash the specified window's frame.
    FlashWindow(WindowId),
    /// Flash the entire screen.
    FlashScreen,
    /// No visual bell (disabled or throttled by safety limits).
    None,
}

/// Trigger a visual bell notification with photosensitivity safety checks.
///
/// Applies photosensitivity rate-limiting to ensure no more than 2Hz flash frequency
/// across the entire desktop. Returns the region to flash, or None if the bell is
/// disabled or throttled.
///
/// # Arguments
/// * `visual_bell_enabled` - User preference for visual (vs. audible) bell
/// * `target_window` - Optional focused window; if None, flashes entire screen
/// * `current_time_ms` - Current monotonic clock time in milliseconds
///
/// # Returns
/// - `BellAction::FlashWindow(id)` if window is focused and flash is allowed
/// - `BellAction::FlashScreen` if no focused window or window flash disabled
/// - `BellAction::None` if visual bell is disabled or safety throttle active
pub fn trigger_bell(
    visual_bell_enabled: bool,
    target_window: Option<WindowId>,
    current_time_ms: u64,
) -> BellAction {
    // If visual bell is not enabled, return immediately.
    if !visual_bell_enabled {
        return BellAction::None;
    }

    // Load the last bell time and compute elapsed time since then.
    let last_time_ms = LAST_VISUAL_BELL_TIME_MS.load(Ordering::Relaxed);
    let elapsed_ms = current_time_ms.saturating_sub(last_time_ms);

    // Photosensitivity safety: skip bells within 500ms of the last one.
    if elapsed_ms < MIN_TIME_BETWEEN_VISUAL_ALERTS_MS {
        return BellAction::None;
    }

    // Update the timestamp for the next check.
    LAST_VISUAL_BELL_TIME_MS.store(current_time_ms, Ordering::Relaxed);

    // Decide flash count: single if recent (< 3s), double if older.
    // (This is for reference; flash count is not encoded in BellAction.)
    let _n_flashes = if elapsed_ms < MIN_TIME_BETWEEN_DOUBLE_VISUAL_ALERT_MS {
        1
    } else {
        2
    };

    // Flash the focused window if available; otherwise, flash the entire screen.
    match target_window {
        Some(window_id) => BellAction::FlashWindow(window_id),
        None => BellAction::FlashScreen,
    }
}

/// Reset the visual bell timer. Useful for testing or after mode transitions.
#[cfg(any(test, feature = "testing"))]
pub fn reset_bell_timer() {
    LAST_VISUAL_BELL_TIME_MS.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_bell_returns_none() {
        let result = trigger_bell(false, Some(WindowId::new(1)), 1000);
        assert_eq!(result, BellAction::None);
    }

    #[test]
    fn first_bell_with_window_flashes_window() {
        reset_bell_timer();
        let result = trigger_bell(true, Some(WindowId::new(42)), 1000);
        assert_eq!(result, BellAction::FlashWindow(WindowId::new(42)));
    }

    #[test]
    fn first_bell_without_window_flashes_screen() {
        reset_bell_timer();
        let result = trigger_bell(true, None, 1000);
        assert_eq!(result, BellAction::FlashScreen);
    }

    #[test]
    fn rapid_bell_throttled() {
        reset_bell_timer();
        trigger_bell(true, Some(WindowId::new(1)), 1000);
        let result = trigger_bell(true, Some(WindowId::new(1)), 1200); // 200ms later
        assert_eq!(result, BellAction::None);
    }

    #[test]
    fn bell_allowed_after_threshold() {
        reset_bell_timer();
        trigger_bell(true, Some(WindowId::new(1)), 1000);
        let result = trigger_bell(true, Some(WindowId::new(1)), 1600); // 600ms later
        assert_eq!(result, BellAction::FlashWindow(WindowId::new(1)));
    }
}
