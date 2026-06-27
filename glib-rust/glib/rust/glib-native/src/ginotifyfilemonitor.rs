//! `ginotifyfilemonitor` matching `gio/inotify/ginotifyfilemonitor.h`.
//!
//! Inotify file monitor: monitors files/directories using Linux inotify.
//! Stubbed in no_std since inotify requires Linux syscalls.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;
use spin::Mutex;

/// Inotify file monitor (mirrors `GInotifyFileMonitor`).
pub struct InotifyFileMonitor {
    pathname: Mutex<String>,
    active: Mutex<bool>,
}

impl InotifyFileMonitor {
    /// Creates a new inotify file monitor.
    pub fn new() -> Self {
        Self {
            pathname: Mutex::new(String::new()),
            active: Mutex::new(false),
        }
    }

    /// Sets the path to monitor.
    pub fn set_path(&self, path: &str) {
        *self.pathname.lock() = path.into();
    }

    /// Returns the monitored path.
    pub fn path(&self) -> String {
        self.pathname.lock().clone()
    }

    /// Returns whether the monitor is active.
    pub fn is_active(&self) -> bool {
        *self.active.lock()
    }

    /// Starts monitoring (mirrors inotify_add_watch).
    /// No-op in our no_std port.
    pub fn start(&self) -> bool {
        *self.active.lock() = true;
        true
    }

    /// Stops monitoring (mirrors inotify_rm_watch).
    pub fn stop(&self) {
        *self.active.lock() = false;
    }
}

impl Default for InotifyFileMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let m = InotifyFileMonitor::new();
        assert!(!m.is_active());
        assert_eq!(m.path(), "");
    }

    #[test]
    fn test_start_stop() {
        let m = InotifyFileMonitor::new();
        assert!(m.start());
        assert!(m.is_active());
        m.stop();
        assert!(!m.is_active());
    }

    #[test]
    fn test_set_path() {
        let m = InotifyFileMonitor::new();
        m.set_path("/tmp/test");
        assert_eq!(m.path(), "/tmp/test");
    }
}
