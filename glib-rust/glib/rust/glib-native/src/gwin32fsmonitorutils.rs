//! `gwin32fsmonitorutils` matching `gio/win32/gwin32fsmonitorutils.h`.
//!
//! Win32 filesystem monitor utilities: private monitor state and lifecycle.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// File monitor alias type (mirrors `GWin32FileMonitorFileAlias`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Win32FileMonitorFileAlias {
    NoAlias = 0,
    LongFilename = 1,
    ShortFilename = 2,
    NoMatchFound = 3,
}

/// Win32 filesystem monitor private state (mirrors `GWin32FSMonitorPrivate`).
#[derive(Debug)]
pub struct Win32FsMonitorPrivate {
    pub buffer_allocated_bytes: u32,
    pub buffer_filled_bytes: u32,
    pub h_directory: usize,
    pub is_file: bool,
    pub wfullpath_with_long_prefix: Vec<u16>,
    pub wfilename_short: Vec<u16>,
    pub wfilename_long: Vec<u16>,
    pub file_attribs: u32,
}

impl Win32FsMonitorPrivate {
    /// Creates a new monitor private (mirrors `g_win32_fs_monitor_create`).
    pub fn new(is_file: bool) -> Self {
        Self {
            buffer_allocated_bytes: 4096,
            buffer_filled_bytes: 0,
            h_directory: 0,
            is_file,
            wfullpath_with_long_prefix: Vec::new(),
            wfilename_short: Vec::new(),
            wfilename_long: Vec::new(),
            file_attribs: 0,
        }
    }

    /// Initializes the monitor with directory and filename
    /// (mirrors `g_win32_fs_monitor_init`).
    pub fn init(&mut self, dirname: &str, filename: &str, is_file: bool) {
        self.is_file = is_file;
        let full_path = if dirname.is_empty() {
            filename.to_string()
        } else if dirname.ends_with('\\') || dirname.ends_with('/') {
            format!("{}{}", dirname, filename)
        } else {
            format!("{}\\{}", dirname, filename)
        };
        self.wfullpath_with_long_prefix = full_path.encode_utf16().collect();
        self.wfullpath_with_long_prefix.push(0);
        self.wfilename_long = filename.encode_utf16().collect();
        self.wfilename_long.push(0);
    }

    /// Finalizes the monitor (mirrors `g_win32_fs_monitor_finalize`).
    pub fn finalize(&mut self) {
        self.wfullpath_with_long_prefix.clear();
        self.wfilename_short.clear();
        self.wfilename_long.clear();
        self.buffer_filled_bytes = 0;
        self.buffer_allocated_bytes = 0;
    }

    /// Closes the directory handle (mirrors `g_win32_fs_monitor_close_handle`).
    pub fn close_handle(&mut self) {
        self.h_directory = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_file_monitor() {
        let m = Win32FsMonitorPrivate::new(true);
        assert!(m.is_file);
        assert_eq!(m.buffer_allocated_bytes, 4096);
    }

    #[test]
    fn test_new_dir_monitor() {
        let m = Win32FsMonitorPrivate::new(false);
        assert!(!m.is_file);
    }

    #[test]
    fn test_init() {
        let mut m = Win32FsMonitorPrivate::new(true);
        m.init("C:\\test", "file.txt", true);
        assert!(!m.wfullpath_with_long_prefix.is_empty());
        assert!(!m.wfilename_long.is_empty());
    }

    #[test]
    fn test_finalize() {
        let mut m = Win32FsMonitorPrivate::new(true);
        m.init("C:\\test", "file.txt", true);
        m.finalize();
        assert!(m.wfullpath_with_long_prefix.is_empty());
    }

    #[test]
    fn test_close_handle() {
        let mut m = Win32FsMonitorPrivate::new(false);
        m.h_directory = 42;
        m.close_handle();
        assert_eq!(m.h_directory, 0);
    }

    #[test]
    fn test_alias_values() {
        assert_eq!(Win32FileMonitorFileAlias::NoAlias as u32, 0);
        assert_eq!(Win32FileMonitorFileAlias::LongFilename as u32, 1);
        assert_eq!(Win32FileMonitorFileAlias::ShortFilename as u32, 2);
        assert_eq!(Win32FileMonitorFileAlias::NoMatchFound as u32, 3);
    }
}
