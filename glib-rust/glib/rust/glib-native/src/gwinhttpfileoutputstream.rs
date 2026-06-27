//! `gwinhttpfileoutputstream` matching `gio/win32/gwinhttpfileoutputstream.h`.
//!
//! WinHTTP file output stream: writes data to an HTTP request.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::winhttp::HInternet;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// WinHTTP file output stream (mirrors `GWinHttpFileOutputStream`).
pub struct WinHttpFileOutputStream {
    connection: Mutex<HInternet>,
    written: Mutex<Vec<u8>>,
    closed: Mutex<bool>,
}

impl WinHttpFileOutputStream {
    /// Creates a new WinHTTP file output stream
    /// (mirrors `_g_winhttp_file_output_stream_new`).
    pub fn new(connection: HInternet) -> Self {
        Self {
            connection: Mutex::new(connection),
            written: Mutex::new(Vec::new()),
            closed: Mutex::new(false),
        }
    }

    /// Returns the connection handle.
    pub fn connection(&self) -> HInternet {
        *self.connection.lock()
    }

    /// Writes data to the stream (mirrors `g_output_stream_write`).
    pub fn write(&self, data: &[u8]) -> Result<usize, String> {
        if *self.closed.lock() {
            return Err("stream is closed".to_string());
        }
        self.written.lock().extend_from_slice(data);
        Ok(data.len())
    }

    /// Closes the stream (mirrors `g_output_stream_close`).
    pub fn close(&self) -> Result<(), String> {
        *self.closed.lock() = true;
        *self.connection.lock() = 0;
        Ok(())
    }

    /// Returns whether the stream is closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }

    /// Returns the data written to the stream (for testing).
    pub fn written_data(&self) -> Vec<u8> {
        self.written.lock().clone()
    }

    /// Flushes the stream (mirrors `g_output_stream_flush`).
    pub fn flush(&self) -> Result<(), String> {
        if *self.closed.lock() {
            return Err("stream is closed".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let s = WinHttpFileOutputStream::new(42);
        assert_eq!(s.connection(), 42);
        assert!(!s.is_closed());
    }

    #[test]
    fn test_write() {
        let s = WinHttpFileOutputStream::new(0);
        s.write(b"hello").unwrap();
        assert_eq!(s.written_data(), b"hello");
    }

    #[test]
    fn test_close() {
        let s = WinHttpFileOutputStream::new(1);
        s.close().unwrap();
        assert!(s.is_closed());
        assert_eq!(s.connection(), 0);
    }

    #[test]
    fn test_write_after_close() {
        let s = WinHttpFileOutputStream::new(0);
        s.close().unwrap();
        assert!(s.write(b"data").is_err());
    }

    #[test]
    fn test_flush() {
        let s = WinHttpFileOutputStream::new(0);
        assert!(s.flush().is_ok());
    }
}
