//! gmemorymonitorwin32 matching `gio/gmemorymonitorwin32.c`.
//!
//! Windows memory monitor that uses `CreateMemoryResourceNotification`
//! to detect low memory conditions and emits warnings via the
//! `GMemoryMonitorInterface`.
//!
//! In this no_std port, we model the memory monitor state machine
//! without actual Windows API calls.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::String;
use spin::Mutex;

/// Memory monitor warning level.
///
/// Mirrors `GMemoryMonitorWarningLevel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryMonitorWarningLevel {
    Low = 0,
    Medium = 50,
    Critical = 100,
}

/// Windows memory monitor (`GMemoryMonitorWin32`).
///
/// In the C implementation, this creates a memory resource notification
/// handle and a background thread that waits for low-memory events.
pub struct MemoryMonitorWin32 {
    initialized: Mutex<bool>,
    warning_level: Mutex<MemoryMonitorWarningLevel>,
}

impl MemoryMonitorWin32 {
    /// Creates a new memory monitor.
    pub fn new() -> Self {
        Self {
            initialized: Mutex::new(false),
            warning_level: Mutex::new(MemoryMonitorWarningLevel::Low),
        }
    }

    /// Initializes the memory monitor.
    ///
    /// In the C implementation, this calls `CreateMemoryResourceNotification`
    /// and starts a watch thread.
    ///
    /// Returns `true` on success.
    pub fn init(&self) -> bool {
        *self.initialized.lock() = true;
        true
    }

    /// Returns whether the monitor is initialized.
    pub fn is_initialized(&self) -> bool {
        *self.initialized.lock()
    }

    /// Gets the current warning level.
    pub fn warning_level(&self) -> MemoryMonitorWarningLevel {
        *self.warning_level.lock()
    }

    /// Sets the warning level (simulating a memory event).
    pub fn set_warning_level(&self, level: MemoryMonitorWarningLevel) {
        *self.warning_level.lock() = level;
    }

    /// Simulates a low-memory event.
    pub fn trigger_low_memory(&self) {
        self.set_warning_level(MemoryMonitorWarningLevel::Low);
    }

    /// Simulates a critical memory event.
    pub fn trigger_critical_memory(&self) {
        self.set_warning_level(MemoryMonitorWarningLevel::Critical);
    }

    /// Resets to normal (no warning).
    pub fn reset(&self) {
        self.set_warning_level(MemoryMonitorWarningLevel::Low);
    }
}

impl Default for MemoryMonitorWin32 {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let monitor = MemoryMonitorWin32::new();
        assert!(!monitor.is_initialized());
        assert!(monitor.init());
        assert!(monitor.is_initialized());
    }

    #[test]
    fn test_warning_level() {
        let monitor = MemoryMonitorWin32::new();
        assert_eq!(monitor.warning_level(), MemoryMonitorWarningLevel::Low);
        monitor.trigger_critical_memory();
        assert_eq!(monitor.warning_level(), MemoryMonitorWarningLevel::Critical);
        monitor.reset();
        assert_eq!(monitor.warning_level(), MemoryMonitorWarningLevel::Low);
    }
}
