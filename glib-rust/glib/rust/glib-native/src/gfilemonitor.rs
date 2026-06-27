//! GFileMonitor matching `gio/gfilemonitor.h`.
//!
//! Upstream `GFileMonitor` monitors files/directories for changes.
//! We port it as a struct with cancellation and rate-limit support.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Events emitted by `GFileMonitor` (`GFileMonitorEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMonitorEvent {
    Changed = 0,
    ChangesDoneHint = 1,
    Deleted = 2,
    Created = 3,
    AttributeChanged = 4,
    PreUnmount = 5,
    Unmounted = 6,
    Moved = 7,
    Renamed = 8,
    MovedIn = 9,
    MovedOut = 10,
}

/// A file monitor (`GFileMonitor`).
pub struct FileMonitor {
    cancelled: Mutex<bool>,
    rate_limit_msecs: Mutex<i32>,
    events: Mutex<Vec<(String, Option<String>, FileMonitorEvent)>>,
}

impl FileMonitor {
    /// Creates a new file monitor.
    pub fn new() -> Self {
        Self {
            cancelled: Mutex::new(false),
            rate_limit_msecs: Mutex::new(800),
            events: Mutex::new(Vec::new()),
        }
    }

    /// Cancels the monitor.
    ///
    /// Mirrors `g_file_monitor_cancel`.
    pub fn cancel(&self) -> bool {
        let was = *self.cancelled.lock();
        *self.cancelled.lock() = true;
        !was
    }

    /// Checks if the monitor is cancelled.
    ///
    /// Mirrors `g_file_monitor_is_cancelled`.
    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.lock()
    }

    /// Sets the rate limit in milliseconds.
    ///
    /// Mirrors `g_file_monitor_set_rate_limit`.
    pub fn set_rate_limit(&self, limit_msecs: i32) {
        *self.rate_limit_msecs.lock() = limit_msecs;
    }

    /// Gets the rate limit.
    pub fn get_rate_limit(&self) -> i32 {
        *self.rate_limit_msecs.lock()
    }

    /// Emits a change event.
    ///
    /// Mirrors `g_file_monitor_emit_event`.
    pub fn emit_event(&self, child: &str, other_file: Option<&str>, event_type: FileMonitorEvent) {
        if self.is_cancelled() {
            return;
        }
        self.events.lock().push((
            child.to_string(),
            other_file.map(|s| s.to_string()),
            event_type,
        ));
    }

    /// Gets all emitted events (for testing/inspection).
    pub fn get_events(&self) -> Vec<(String, Option<String>, FileMonitorEvent)> {
        self.events.lock().clone()
    }
}

impl Default for FileMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let monitor = FileMonitor::new();
        assert!(!monitor.is_cancelled());
        assert_eq!(monitor.get_rate_limit(), 800);
    }

    #[test]
    fn test_cancel() {
        let monitor = FileMonitor::new();
        assert!(monitor.cancel());
        assert!(monitor.is_cancelled());
        assert!(!monitor.cancel());
    }

    #[test]
    fn test_set_rate_limit() {
        let monitor = FileMonitor::new();
        monitor.set_rate_limit(1000);
        assert_eq!(monitor.get_rate_limit(), 1000);
    }

    #[test]
    fn test_emit_event() {
        let monitor = FileMonitor::new();
        monitor.emit_event("/test/file.txt", None, FileMonitorEvent::Changed);
        monitor.emit_event(
            "/test/file.txt",
            Some("/test/renamed.txt"),
            FileMonitorEvent::Renamed,
        );
        let events = monitor.get_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "/test/file.txt");
        assert_eq!(events[0].2, FileMonitorEvent::Changed);
        assert_eq!(events[1].1.as_ref().unwrap(), "/test/renamed.txt");
        assert_eq!(events[1].2, FileMonitorEvent::Renamed);
    }

    #[test]
    fn test_emit_after_cancel() {
        let monitor = FileMonitor::new();
        monitor.cancel();
        monitor.emit_event("/test/file.txt", None, FileMonitorEvent::Changed);
        assert!(monitor.get_events().is_empty());
    }

    #[test]
    fn test_event_values() {
        assert_eq!(FileMonitorEvent::Changed as u8, 0);
        assert_eq!(FileMonitorEvent::Deleted as u8, 2);
        assert_eq!(FileMonitorEvent::Created as u8, 3);
        assert_eq!(FileMonitorEvent::Renamed as u8, 8);
    }
}
