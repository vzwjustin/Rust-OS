//! GLocalFileMonitor matching `gio/glocalfilemonitor.h`.
//! A file monitor for local files. In this no_std port we model it
//! with a path and changed flag.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// A local file monitor (`GLocalFileMonitor`).
pub struct LocalFileMonitor {
    path: Mutex<String>,
    changed: Mutex<bool>,
    cancelled: Mutex<bool>,
}

impl LocalFileMonitor {
    pub fn new(path: &str) -> Self {
        Self {
            path: Mutex::new(path.to_string()),
            changed: Mutex::new(false),
            cancelled: Mutex::new(false),
        }
    }

    pub fn get_path(&self) -> String {
        self.path.lock().clone()
    }
    pub fn has_changed(&self) -> bool {
        let c = *self.changed.lock();
        *self.changed.lock() = false;
        c
    }
    pub fn notify_changed(&self) {
        *self.changed.lock() = true;
    }
    pub fn cancel(&self) {
        *self.cancelled.lock() = true;
    }
    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor() {
        let m = LocalFileMonitor::new("/tmp/test.txt");
        m.notify_changed();
        assert!(m.has_changed());
        assert!(!m.has_changed()); // flag cleared
    }
}
