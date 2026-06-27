//! `gkqueuefilemonitor` matching `gio/kqueue/gkqueuefilemonitor.h`.
//!
//! Kqueue file monitor: monitors files/directories using BSD kqueue.
//! Stubbed in no_std since kqueue requires BSD syscalls.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::String;
use spin::Mutex;

/// Kqueue file monitor (mirrors `GKqueueFileMonitor`).
pub struct KqueueFileMonitor {
    filename: Mutex<String>,
    fd: Mutex<i32>,
    active: Mutex<bool>,
}

impl KqueueFileMonitor {
    /// Creates a new kqueue file monitor.
    pub fn new() -> Self {
        Self {
            filename: Mutex::new(String::new()),
            fd: Mutex::new(-1),
            active: Mutex::new(false),
        }
    }

    /// Sets the filename to monitor.
    pub fn set_filename(&self, filename: &str) {
        *self.filename.lock() = filename.into();
    }

    /// Returns the monitored filename.
    pub fn filename(&self) -> String {
        self.filename.lock().clone()
    }

    /// Returns the file descriptor.
    pub fn fd(&self) -> i32 {
        *self.fd.lock()
    }

    /// Returns whether the monitor is active.
    pub fn is_active(&self) -> bool {
        *self.active.lock()
    }

    /// Starts monitoring (mirrors kqueue kevent registration).
    /// No-op in our no_std port.
    pub fn start(&self) -> bool {
        *self.active.lock() = true;
        true
    }

    /// Stops monitoring.
    pub fn stop(&self) {
        *self.active.lock() = false;
        *self.fd.lock() = -1;
    }
}

impl Default for KqueueFileMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let m = KqueueFileMonitor::new();
        assert!(!m.is_active());
        assert_eq!(m.fd(), -1);
    }

    #[test]
    fn test_start_stop() {
        let m = KqueueFileMonitor::new();
        assert!(m.start());
        assert!(m.is_active());
        m.stop();
        assert!(!m.is_active());
        assert_eq!(m.fd(), -1);
    }

    #[test]
    fn test_set_filename() {
        let m = KqueueFileMonitor::new();
        m.set_filename("/tmp/test");
        assert_eq!(m.filename(), "/tmp/test");
    }
}
