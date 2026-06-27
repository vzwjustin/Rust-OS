//! GPollFileMonitor matching `gio/gpollfilemonitor.h`.
//! A file monitor that uses polling. In this no_std port we model it
//! with a file path and last-modified timestamp.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// A poll-based file monitor (`GPollFileMonitor`).
pub struct PollFileMonitor {
    filename: Mutex<String>,
    last_mtime: Mutex<u64>,
    cancelled: Mutex<bool>,
}

impl PollFileMonitor {
    pub fn new(filename: &str) -> Self {
        Self {
            filename: Mutex::new(filename.to_string()),
            last_mtime: Mutex::new(0),
            cancelled: Mutex::new(false),
        }
    }

    pub fn get_filename(&self) -> String {
        self.filename.lock().clone()
    }

    pub fn poll(&self, mtime: u64) -> bool {
        if *self.cancelled.lock() {
            return false;
        }
        let mut last = self.last_mtime.lock();
        let changed = *last != mtime;
        *last = mtime;
        changed
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
    fn test_poll_detects_change() {
        let m = PollFileMonitor::new("/tmp/test.txt");
        assert!(m.poll(1000)); // first poll always detects
        assert!(!m.poll(1000)); // same mtime, no change
        assert!(m.poll(2000)); // mtime changed
    }

    #[test]
    fn test_cancel() {
        let m = PollFileMonitor::new("/tmp/test.txt");
        m.cancel();
        assert!(m.is_cancelled());
        assert!(!m.poll(1000));
    }
}
