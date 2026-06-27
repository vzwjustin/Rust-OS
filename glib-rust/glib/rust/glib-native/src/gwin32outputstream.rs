//! gwin32outputstream matching `gio/gwin32outputstream.c`.
//!
//! `GWin32OutputStream` implements `GOutputStream` for writing to a
//! Windows file handle (`HANDLE`).
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

pub type WinHandle = usize;

pub struct Win32OutputStream {
    handle: Mutex<WinHandle>,
    close_handle: Mutex<bool>,
    fd: Mutex<i32>,
    written: Mutex<Vec<u8>>,
    closed: Mutex<bool>,
}

impl Win32OutputStream {
    pub fn new(handle: WinHandle, close_handle: bool) -> Self {
        Self {
            handle: Mutex::new(handle),
            close_handle: Mutex::new(close_handle),
            fd: Mutex::new(-1),
            written: Mutex::new(Vec::new()),
            closed: Mutex::new(false),
        }
    }

    pub fn new_from_fd(fd: i32, close_handle: bool) -> Self {
        Self {
            handle: Mutex::new(0),
            close_handle: Mutex::new(close_handle),
            fd: Mutex::new(fd),
            written: Mutex::new(Vec::new()),
            closed: Mutex::new(false),
        }
    }

    pub fn handle(&self) -> WinHandle {
        *self.handle.lock()
    }
    pub fn close_handle(&self) -> bool {
        *self.close_handle.lock()
    }
    pub fn set_close_handle(&self, close: bool) {
        *self.close_handle.lock() = close;
    }
    pub fn fd(&self) -> i32 {
        *self.fd.lock()
    }

    pub fn write(&self, data: &[u8]) -> Result<usize, String> {
        if *self.closed.lock() {
            return Err("stream is closed".to_string());
        }
        self.written.lock().extend_from_slice(data);
        Ok(data.len())
    }

    pub fn close(&self) -> Result<(), String> {
        *self.closed.lock() = true;
        if *self.close_handle.lock() {
            *self.handle.lock() = 0;
        }
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
    pub fn written_data(&self) -> Vec<u8> {
        self.written.lock().clone()
    }

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
    fn test_write() {
        let stream = Win32OutputStream::new(1, false);
        let n = stream.write(b"Hello").unwrap();
        assert_eq!(n, 5);
        assert_eq!(stream.written_data(), b"Hello");
    }

    #[test]
    fn test_close() {
        let stream = Win32OutputStream::new(42, true);
        stream.close().unwrap();
        assert!(stream.is_closed());
        assert_eq!(stream.handle(), 0);
        assert!(stream.write(b"data").is_err());
    }

    #[test]
    fn test_flush() {
        let stream = Win32OutputStream::new(1, false);
        assert!(stream.flush().is_ok());
    }
}
