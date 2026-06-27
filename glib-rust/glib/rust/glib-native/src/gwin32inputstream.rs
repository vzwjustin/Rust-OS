//! gwin32inputstream matching `gio/gwin32inputstream.c`.
//!
//! `GWin32InputStream` implements `GInputStream` for reading from a
//! Windows file handle (`HANDLE`). It supports closing the handle
//! when the stream is closed.
//!
//! In this no_std port, we model the stream with a handle ID and
//! an in-memory read buffer.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// A Windows file handle (opaque).
pub type WinHandle = usize;

/// Windows input stream (`GWin32InputStream`).
pub struct Win32InputStream {
    handle: Mutex<WinHandle>,
    close_handle: Mutex<bool>,
    fd: Mutex<i32>,
    buffer: Mutex<Vec<u8>>,
    closed: Mutex<bool>,
}

impl Win32InputStream {
    /// Creates a new `Win32InputStream` for the given handle.
    ///
    /// Mirrors `g_win32_input_stream_new`.
    pub fn new(handle: WinHandle, close_handle: bool) -> Self {
        Self {
            handle: Mutex::new(handle),
            close_handle: Mutex::new(close_handle),
            fd: Mutex::new(-1),
            buffer: Mutex::new(Vec::new()),
            closed: Mutex::new(false),
        }
    }

    /// Creates a new `Win32InputStream` from a file descriptor.
    ///
    /// Mirrors `g_win32_input_stream_new_from_fd`.
    pub fn new_from_fd(fd: i32, close_handle: bool) -> Self {
        Self {
            handle: Mutex::new(0),
            close_handle: Mutex::new(close_handle),
            fd: Mutex::new(fd),
            buffer: Mutex::new(Vec::new()),
            closed: Mutex::new(false),
        }
    }

    /// Returns the handle.
    pub fn handle(&self) -> WinHandle {
        *self.handle.lock()
    }

    /// Returns whether the handle will be closed when the stream is closed.
    pub fn close_handle(&self) -> bool {
        *self.close_handle.lock()
    }

    /// Sets whether to close the handle when the stream is closed.
    pub fn set_close_handle(&self, close: bool) {
        *self.close_handle.lock() = close;
    }

    /// Returns the file descriptor.
    pub fn fd(&self) -> i32 {
        *self.fd.lock()
    }

    /// Appends data to the internal read buffer (simulating a read from the handle).
    pub fn push_data(&self, data: &[u8]) {
        self.buffer.lock().extend_from_slice(data);
    }

    /// Reads data from the stream.
    ///
    /// Mirrors `g_input_stream_read`.
    pub fn read(&self, buffer: &mut [u8]) -> Result<usize, String> {
        if *self.closed.lock() {
            return Err("stream is closed".to_string());
        }
        let mut buf = self.buffer.lock();
        let n = core::cmp::min(buffer.len(), buf.len());
        if n == 0 {
            return Ok(0);
        }
        buffer[..n].copy_from_slice(&buf[..n]);
        buf.drain(..n);
        Ok(n)
    }

    /// Closes the stream.
    ///
    /// Mirrors `g_input_stream_close`.
    pub fn close(&self) -> Result<(), String> {
        *self.closed.lock() = true;
        if *self.close_handle.lock() {
            *self.handle.lock() = 0;
        }
        Ok(())
    }

    /// Returns whether the stream is closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let stream = Win32InputStream::new(42, true);
        assert_eq!(stream.handle(), 42);
        assert!(stream.close_handle());
        assert!(!stream.is_closed());
    }

    #[test]
    fn test_new_from_fd() {
        let stream = Win32InputStream::new_from_fd(3, false);
        assert_eq!(stream.fd(), 3);
        assert!(!stream.close_handle());
    }

    #[test]
    fn test_read() {
        let stream = Win32InputStream::new(1, false);
        stream.push_data(b"Hello");
        let mut buf = [0u8; 10];
        let n = stream.read(&mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..5], b"Hello");
    }

    #[test]
    fn test_read_empty() {
        let stream = Win32InputStream::new(1, false);
        let mut buf = [0u8; 10];
        let n = stream.read(&mut buf).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn test_close() {
        let stream = Win32InputStream::new(42, true);
        stream.close().unwrap();
        assert!(stream.is_closed());
        assert_eq!(stream.handle(), 0);

        let mut buf = [0u8; 10];
        assert!(stream.read(&mut buf).is_err());
    }

    #[test]
    fn test_set_close_handle() {
        let stream = Win32InputStream::new(42, false);
        stream.set_close_handle(true);
        assert!(stream.close_handle());
    }
}
