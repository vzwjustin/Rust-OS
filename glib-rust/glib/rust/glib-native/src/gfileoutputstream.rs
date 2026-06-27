//! GFileOutputStream matching `gio/gfileoutputstream.h`.
//!
//! Upstream `GFileOutputStream` is a `GOutputStream` for writing to
//! a file, with seek, truncate, query_info, and etag support.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::gfile::{FileInfo, FileType};
use crate::gseekable::SeekType;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A file output stream (`GFileOutputStream`).
pub struct FileOutputStream {
    data: Mutex<Vec<u8>>,
    position: Mutex<usize>,
    closed: Mutex<bool>,
    etag: Mutex<Option<String>>,
}

impl FileOutputStream {
    /// Creates a new empty file output stream.
    pub fn new() -> Self {
        Self {
            data: Mutex::new(Vec::new()),
            position: Mutex::new(0),
            closed: Mutex::new(false),
            etag: Mutex::new(None),
        }
    }

    /// Creates a file output stream from initial data.
    pub fn from_data(data: &[u8]) -> Self {
        Self {
            data: Mutex::new(data.to_vec()),
            position: Mutex::new(0),
            closed: Mutex::new(false),
            etag: Mutex::new(None),
        }
    }

    pub fn tell(&self) -> i64 {
        *self.position.lock() as i64
    }

    pub fn can_seek(&self) -> bool {
        true
    }

    pub fn can_truncate(&self) -> bool {
        true
    }

    pub fn seek(
        &self,
        offset: i64,
        type_: SeekType,
        _cancellable: Option<&GCancellable>,
    ) -> Result<(), Error> {
        let mut pos = self.position.lock();
        let data_len = self.data.lock().len();
        let new_pos = match type_ {
            SeekType::Cur => *pos as i64 + offset,
            SeekType::Set => offset,
            SeekType::End => data_len as i64 + offset,
        };
        if new_pos < 0 || new_pos as usize > data_len {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::InvalidArgument.to_code(),
                "Seek position out of bounds",
            ));
        }
        *pos = new_pos as usize;
        Ok(())
    }

    pub fn truncate(&self, size: i64, _cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if size < 0 {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::InvalidArgument.to_code(),
                "Truncate size cannot be negative",
            ));
        }
        let mut data = self.data.lock();
        data.truncate(size as usize);
        let mut pos = self.position.lock();
        if *pos > data.len() {
            *pos = data.len();
        }
        Ok(())
    }

    pub fn query_info(&self, _attributes: &str) -> Result<FileInfo, Error> {
        let mut info = FileInfo::new();
        let data = self.data.lock();
        info.set_size(data.len() as u64);
        info.set_file_type(FileType::Regular);
        Ok(info)
    }

    pub fn get_etag(&self) -> Option<String> {
        self.etag.lock().clone()
    }

    pub fn write(&self, buf: &[u8], _cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        if *self.closed.lock() {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        let mut data = self.data.lock();
        let mut pos = self.position.lock();
        if *pos + buf.len() > data.len() {
            data.resize(*pos + buf.len(), 0);
        }
        data[*pos..*pos + buf.len()].copy_from_slice(buf);
        *pos += buf.len();
        Ok(buf.len())
    }

    pub fn close(&self, _cancellable: Option<&GCancellable>) -> Result<(), Error> {
        *self.closed.lock() = true;
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }

    pub fn get_data(&self) -> Vec<u8> {
        self.data.lock().clone()
    }
}

impl Default for FileOutputStream {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_output_stream_new() {
        let stream = FileOutputStream::new();
        assert_eq!(stream.tell(), 0);
        assert!(stream.can_seek());
        assert!(stream.can_truncate());
        assert!(!stream.is_closed());
    }

    #[test]
    fn test_write() {
        let stream = FileOutputStream::new();
        let n = stream.write(b"hello", None).unwrap();
        assert_eq!(n, 5);
        assert_eq!(stream.tell(), 5);
        assert_eq!(stream.get_data(), b"hello");
    }

    #[test]
    fn test_write_at_position() {
        let stream = FileOutputStream::from_data(b"hello world");
        stream.seek(6, SeekType::Set, None).unwrap();
        stream.write(b"Rust", None).unwrap();
        assert_eq!(stream.get_data(), b"hello Rustd");
    }

    #[test]
    fn test_seek_set() {
        let stream = FileOutputStream::from_data(b"hello world");
        stream.seek(6, SeekType::Set, None).unwrap();
        assert_eq!(stream.tell(), 6);
    }

    #[test]
    fn test_seek_end() {
        let stream = FileOutputStream::from_data(b"hello world");
        stream.seek(-5, SeekType::End, None).unwrap();
        assert_eq!(stream.tell(), 6);
    }

    #[test]
    fn test_truncate() {
        let stream = FileOutputStream::from_data(b"hello world");
        stream.truncate(5, None).unwrap();
        assert_eq!(stream.get_data(), b"hello");
    }

    #[test]
    fn test_close() {
        let stream = FileOutputStream::new();
        stream.close(None).unwrap();
        assert!(stream.is_closed());
        assert!(stream.write(b"data", None).is_err());
    }

    #[test]
    fn test_query_info() {
        let stream = FileOutputStream::from_data(b"hello world");
        let info = stream.query_info("standard::*").unwrap();
        assert_eq!(info.get_size(), 11);
    }

    #[test]
    fn test_etag() {
        let stream = FileOutputStream::new();
        assert!(stream.get_etag().is_none());
    }
}
