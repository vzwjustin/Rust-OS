//! `gwin32filemonitor` matching `gio/win32/gwin32filemonitor.h`.
//!
//! Win32 file monitor: monitors files and directories for changes using
//! Win32 ReadDirectoryChangesW (stubbed in no_std).
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gwin32fsmonitorutils::Win32FsMonitorPrivate;
use crate::prelude::*;
use alloc::string::String;
use spin::Mutex;

/// Win32 file monitor (mirrors `GWin32FileMonitor`).
pub struct Win32FileMonitor {
    priv_: Mutex<Win32FsMonitorPrivate>,
}

impl Win32FileMonitor {
    /// Creates a new Win32 file monitor.
    pub fn new() -> Self {
        Self {
            priv_: Mutex::new(Win32FsMonitorPrivate::new(false)),
        }
    }

    /// Registers the file monitor (mirrors `g_win32_file_monitor_register`).
    /// No-op in our no_std port.
    pub fn register() {}

    /// Initializes the monitor for a directory and filename.
    pub fn init(&self, dirname: &str, filename: &str, is_file: bool) {
        self.priv_.lock().init(dirname, filename, is_file);
    }

    /// Finalizes the monitor.
    pub fn finalize(&self) {
        self.priv_.lock().finalize();
    }

    /// Closes the directory handle.
    pub fn close_handle(&self) {
        self.priv_.lock().close_handle();
    }
}

impl Default for Win32FileMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let m = Win32FileMonitor::new();
        m.init("C:\\test", "file.txt", true);
        m.finalize();
    }

    #[test]
    fn test_register_noop() {
        Win32FileMonitor::register();
    }

    #[test]
    fn test_close_handle() {
        let m = Win32FileMonitor::new();
        m.close_handle();
    }
}
