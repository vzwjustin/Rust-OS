//! gwin32file_sync_stream matching `gio/gwin32file-sync-stream.c`.
//!
//! A COM `IStream` implementation backed by a Windows file `HANDLE`.
//! Provides synchronous read/write/seek operations. Used by
//! `gwin32packageparser` for reading UWP package manifests.
//!
//! In this no_std port, we model the stream with a handle ID and an
//! in-memory buffer (no actual Win32 `ReadFile` / `WriteFile` calls).
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gwin32inputstream::WinHandle;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Seek origin (maps to `STREAM_SEEK_*` / `FILE_*` constants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekOrigin {
    Begin,
    Current,
    End,
}

/// STGM mode flags (reported to third parties; not enforced in the stub).
pub type StgmMode = u32;

pub const STGM_READ: StgmMode = 0x0000_0000;
pub const STGM_WRITE: StgmMode = 0x0000_0001;
pub const STGM_READWRITE: StgmMode = 0x0000_0002;

/// A synchronous file stream (`GWin32FileSyncStream`).
pub struct Win32FileSyncStream {
    handle: Mutex<WinHandle>,
    close_handle: Mutex<bool>,
    stgm_mode: Mutex<StgmMode>,
    buffer: Mutex<Vec<u8>>,
    position: Mutex<u64>,
    ref_count: Mutex<u32>,
    closed: Mutex<bool>,
}

impl Win32FileSyncStream {
    /// Creates a stream backed by `handle`.
    ///
    /// Mirrors `g_win32_file_sync_stream_new`.
    pub fn new(handle: WinHandle, close_handle: bool) -> Self {
        Self::new_with_mode(handle, close_handle, STGM_READWRITE)
    }

    /// Creates a stream with an explicit STGM mode.
    pub fn new_with_mode(handle: WinHandle, close_handle: bool, stgm_mode: StgmMode) -> Self {
        Self {
            handle: Mutex::new(handle),
            close_handle: Mutex::new(close_handle),
            stgm_mode: Mutex::new(stgm_mode),
            buffer: Mutex::new(Vec::new()),
            position: Mutex::new(0),
            ref_count: Mutex::new(1),
            closed: Mutex::new(false),
        }
    }

    /// Creates a stream from in-memory data (testing helper).
    pub fn from_data(data: Vec<u8>) -> Self {
        let stream = Self::new(1, false);
        *stream.buffer.lock() = data;
        stream
    }

    pub fn handle(&self) -> WinHandle {
        *self.handle.lock()
    }

    pub fn close_handle(&self) -> bool {
        *self.close_handle.lock()
    }

    pub fn stgm_mode(&self) -> StgmMode {
        *self.stgm_mode.lock()
    }

    /// Preloads data into the internal buffer (simulates file contents).
    pub fn preload(&self, data: &[u8]) {
        self.buffer.lock().extend_from_slice(data);
    }

    pub fn add_ref(&self) -> u32 {
        let mut rc = self.ref_count.lock();
        *rc += 1;
        *rc
    }

    pub fn release(&self) -> u32 {
        let mut rc = self.ref_count.lock();
        if *rc > 0 {
            *rc -= 1;
        }
        *rc
    }

    pub fn read(&self, out: &mut [u8]) -> Result<usize, String> {
        if *self.closed.lock() {
            return Err("stream is closed".to_string());
        }
        let data = self.buffer.lock();
        let pos = *self.position.lock() as usize;
        if pos >= data.len() {
            return Ok(0);
        }
        let n = core::cmp::min(out.len(), data.len() - pos);
        out[..n].copy_from_slice(&data[pos..pos + n]);
        *self.position.lock() += n as u64;
        Ok(n)
    }

    pub fn write(&self, data: &[u8]) -> Result<usize, String> {
        if *self.closed.lock() {
            return Err("stream is closed".to_string());
        }
        let mut buf = self.buffer.lock();
        let pos = *self.position.lock() as usize;
        let end = pos + data.len();
        if end > buf.len() {
            buf.resize(end, 0);
        }
        buf[pos..end].copy_from_slice(data);
        *self.position.lock() += data.len() as u64;
        Ok(data.len())
    }

    pub fn seek(&self, offset: i64, origin: SeekOrigin) -> Result<u64, String> {
        if *self.closed.lock() {
            return Err("stream is closed".to_string());
        }
        let len = self.buffer.lock().len() as i64;
        let new_pos = match origin {
            SeekOrigin::Begin => offset,
            SeekOrigin::Current => *self.position.lock() as i64 + offset,
            SeekOrigin::End => len + offset,
        };
        if new_pos < 0 {
            return Err("negative position".to_string());
        }
        *self.position.lock() = new_pos as u64;
        Ok(new_pos as u64)
    }

    /// Returns the current stream position (`tell`).
    pub fn tell(&self) -> u64 {
        *self.position.lock()
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

    pub fn size(&self) -> u64 {
        self.buffer.lock().len() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_and_handle() {
        let stream = Win32FileSyncStream::new(42, true);
        assert_eq!(stream.handle(), 42);
        assert!(stream.close_handle());
        assert_eq!(stream.stgm_mode(), STGM_READWRITE);
    }

    #[test]
    fn test_read_write_seek_tell() {
        let stream = Win32FileSyncStream::from_data(b"Hello World".to_vec());
        let mut buf = [0u8; 5];
        assert_eq!(stream.read(&mut buf).unwrap(), 5);
        assert_eq!(&buf, b"Hello");
        assert_eq!(stream.tell(), 5);

        stream.seek(6, SeekOrigin::Begin).unwrap();
        assert_eq!(stream.tell(), 6);
        stream.read(&mut buf).unwrap();
        assert_eq!(&buf, b"World");
    }

    #[test]
    fn test_write_extends_buffer() {
        let stream = Win32FileSyncStream::new(1, false);
        stream.write(b"abc").unwrap();
        assert_eq!(stream.size(), 3);
        stream.seek(0, SeekOrigin::Begin).unwrap();
        let mut buf = [0u8; 3];
        stream.read(&mut buf).unwrap();
        assert_eq!(&buf, b"abc");
    }

    #[test]
    fn test_seek_from_end() {
        let stream = Win32FileSyncStream::from_data(b"Hello".to_vec());
        stream.seek(-2, SeekOrigin::End).unwrap();
        assert_eq!(stream.tell(), 3);
    }

    #[test]
    fn test_close_clears_handle() {
        let stream = Win32FileSyncStream::new(99, true);
        stream.close().unwrap();
        assert!(stream.is_closed());
        assert_eq!(stream.handle(), 0);
        let mut buf = [0u8; 1];
        assert!(stream.read(&mut buf).is_err());
    }

    #[test]
    fn test_ref_count() {
        let stream = Win32FileSyncStream::new(1, false);
        assert_eq!(stream.add_ref(), 2);
        assert_eq!(stream.release(), 1);
    }

    #[test]
    fn test_preload() {
        let stream = Win32FileSyncStream::new(1, false);
        stream.preload(b"data");
        assert_eq!(stream.size(), 4);
    }
}
