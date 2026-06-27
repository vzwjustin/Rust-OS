//! `gwinhttpfileinputstream` matching `gio/win32/gwinhttpfileinputstream.h`.
//!
//! WinHTTP file input stream: reads data from an HTTP response.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::winhttp::HInternet;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// WinHTTP file input stream (mirrors `GWinHttpFileInputStream`).
pub struct WinHttpFileInputStream {
    connection: Mutex<HInternet>,
    request: Mutex<HInternet>,
    buffer: Mutex<Vec<u8>>,
    closed: Mutex<bool>,
}

impl WinHttpFileInputStream {
    /// Creates a new WinHTTP file input stream
    /// (mirrors `_g_winhttp_file_input_stream_new`).
    pub fn new(connection: HInternet, request: HInternet) -> Self {
        Self {
            connection: Mutex::new(connection),
            request: Mutex::new(request),
            buffer: Mutex::new(Vec::new()),
            closed: Mutex::new(false),
        }
    }

    /// Returns the connection handle.
    pub fn connection(&self) -> HInternet {
        *self.connection.lock()
    }

    /// Returns the request handle.
    pub fn request(&self) -> HInternet {
        *self.request.lock()
    }

    /// Pushes data into the internal buffer (for testing).
    pub fn push_data(&self, data: &[u8]) {
        self.buffer.lock().extend_from_slice(data);
    }

    /// Reads data from the stream (mirrors `g_input_stream_read`).
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, String> {
        if *self.closed.lock() {
            return Err("stream is closed".to_string());
        }
        let mut buffer = self.buffer.lock();
        let len = buf.len().min(buffer.len());
        if len == 0 {
            return Ok(0);
        }
        buf[..len].copy_from_slice(&buffer[..len]);
        buffer.drain(..len);
        Ok(len)
    }

    /// Closes the stream (mirrors `g_input_stream_close`).
    pub fn close(&self) -> Result<(), String> {
        *self.closed.lock() = true;
        *self.connection.lock() = 0;
        *self.request.lock() = 0;
        Ok(())
    }

    /// Returns whether the stream is closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let s = WinHttpFileInputStream::new(1, 2);
        assert_eq!(s.connection(), 1);
        assert_eq!(s.request(), 2);
        assert!(!s.is_closed());
    }

    #[test]
    fn test_read() {
        let s = WinHttpFileInputStream::new(0, 0);
        s.push_data(b"hello");
        let mut buf = [0u8; 10];
        let n = s.read(&mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..5], b"hello");
    }

    #[test]
    fn test_close() {
        let s = WinHttpFileInputStream::new(1, 2);
        s.close().unwrap();
        assert!(s.is_closed());
        assert_eq!(s.connection(), 0);
    }

    #[test]
    fn test_read_after_close() {
        let s = WinHttpFileInputStream::new(0, 0);
        s.close().unwrap();
        let mut buf = [0u8; 10];
        assert!(s.read(&mut buf).is_err());
    }
}
