//! `giowin32-priv` matching `gio/giowin32-priv.h`.
//!
//! Private Win32 I/O API: input/output stream creation from fd,
//! and appinfo initialization.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gwin32inputstream::Win32InputStream;
use crate::gwin32outputstream::Win32OutputStream;
use crate::prelude::*;
use alloc::string::String;

/// Creates a `Win32InputStream` from a file descriptor
/// (mirrors `g_win32_input_stream_new_from_fd`).
pub fn win32_input_stream_new_from_fd(fd: i32, close_fd: bool) -> Win32InputStream {
    Win32InputStream::new_from_fd(fd, close_fd)
}

/// Creates a `Win32OutputStream` from a file descriptor
/// (mirrors `g_win32_output_stream_new_from_fd`).
pub fn win32_output_stream_new_from_fd(fd: i32, close_fd: bool) -> Win32OutputStream {
    Win32OutputStream::new_from_fd(fd, close_fd)
}

/// Initializes Win32 appinfo (mirrors `gio_win32_appinfo_init`).
///
/// In our no_std port, this is a no-op.
pub fn win32_appinfo_init(_do_wait: bool) {
    // No-op: Win32 app info initialization is not needed in our port.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_stream_from_fd() {
        let stream = win32_input_stream_new_from_fd(3, false);
        assert_eq!(stream.fd(), 3);
        assert!(!stream.close_handle());
    }

    #[test]
    fn test_output_stream_from_fd() {
        let stream = win32_output_stream_new_from_fd(4, true);
        assert_eq!(stream.fd(), 4);
        assert!(stream.close_handle());
    }

    #[test]
    fn test_appinfo_init_noop() {
        win32_appinfo_init(true);
        win32_appinfo_init(false);
    }
}
