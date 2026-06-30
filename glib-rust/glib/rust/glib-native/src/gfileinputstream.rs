//! GFileInputStream matching `gio/gfileinputstream.h`.
//!
//! Upstream `GFileInputStream` is a `GInputStream` for reading from
//! a file, with seek support and `query_info`. We port it as a struct
//! wrapping a `MemoryInputStream` with seek support.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::gfile::FileInfo;
use crate::gseekable::SeekType;
use alloc::vec::Vec;
use spin::Mutex;

/// A file input stream (`GFileInputStream`).
///
/// Wraps an in-memory buffer with seek support, simulating file I/O.
pub struct FileInputStream {
    data: Vec<u8>,
    position: Mutex<usize>,
    closed: Mutex<bool>,
}

impl FileInputStream {
    /// Creates a file input stream from a byte buffer.
    pub fn from_data(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
            position: Mutex::new(0),
            closed: Mutex::new(false),
        }
    }

    /// Gets the current position.
    ///
    /// Mirrors `g_seekable_tell`.
    pub fn tell(&self) -> i64 {
        *self.position.lock() as i64
    }

    /// Checks if seeking is supported.
    ///
    /// Mirrors `g_seekable_can_seek`.
    pub fn can_seek(&self) -> bool {
        true
    }

    /// Seeks within the stream.
    ///
    /// Mirrors `g_seekable_seek`.
    pub fn seek(
        &self,
        offset: i64,
        type_: SeekType,
        _cancellable: Option<&GCancellable>,
    ) -> Result<(), Error> {
        let mut pos = self.position.lock();
        let new_pos = match type_ {
            SeekType::Cur => *pos as i64 + offset,
            SeekType::Set => offset,
            SeekType::End => self.data.len() as i64 + offset,
        };
        if new_pos < 0 || new_pos as usize > self.data.len() {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::InvalidArgument.to_code(),
                "Seek position out of bounds",
            ));
        }
        *pos = new_pos as usize;
        Ok(())
    }

    /// Queries file info.
    ///
    /// Mirrors `g_file_input_stream_query_info`.
    pub fn query_info(&self, _attributes: &str) -> Result<FileInfo, Error> {
        let mut info = FileInfo::new();
        info.set_size(self.data.len() as u64);
        info.set_file_type(crate::gfile::FileType::Regular);
        Ok(info)
    }

    /// Reads data from the current position.
    pub fn read(
        &self,
        buf: &mut [u8],
        _cancellable: Option<&GCancellable>,
    ) -> Result<usize, Error> {
        if *self.closed.lock() {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        let mut pos = self.position.lock();
        let available = self.data.len().saturating_sub(*pos);
        let to_read = buf.len().min(available);
        if to_read == 0 {
            return Ok(0);
        }
        buf[..to_read].copy_from_slice(&self.data[*pos..*pos + to_read]);
        *pos += to_read;
        Ok(to_read)
    }

    /// Closes the stream.
    pub fn close(&self, _cancellable: Option<&GCancellable>) -> Result<(), Error> {
        *self.closed.lock() = true;
        Ok(())
    }

    /// Checks if the stream is closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_input_stream_from_data() {
        let stream = FileInputStream::from_data(b"hello world");
        assert_eq!(stream.tell(), 0);
        assert!(stream.can_seek());
        assert!(!stream.is_closed());
    }

    #[test]
    fn test_read() {
        let stream = FileInputStream::from_data(b"hello world");
        let mut buf = [0u8; 5];
        let n = stream.read(&mut buf, None).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");
        assert_eq!(stream.tell(), 5);
    }

    #[test]
    fn test_read_partial() {
        let stream = FileInputStream::from_data(b"hello");
        let mut buf = [0u8; 10];
        let n = stream.read(&mut buf, None).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..5], b"hello");
    }

    #[test]
    fn test_seek_set() {
        let stream = FileInputStream::from_data(b"hello world");
        stream.seek(6, SeekType::Set, None).unwrap();
        assert_eq!(stream.tell(), 6);
        let mut buf = [0u8; 5];
        stream.read(&mut buf, None).unwrap();
        assert_eq!(&buf, b"world");
    }

    #[test]
    fn test_seek_cur() {
        let stream = FileInputStream::from_data(b"hello world");
        let mut buf = [0u8; 3];
        stream.read(&mut buf, None).unwrap();
        assert_eq!(stream.tell(), 3);
        stream.seek(2, SeekType::Cur, None).unwrap();
        assert_eq!(stream.tell(), 5);
    }

    #[test]
    fn test_seek_end() {
        let stream = FileInputStream::from_data(b"hello world");
        stream.seek(-5, SeekType::End, None).unwrap();
        assert_eq!(stream.tell(), 6);
    }

    #[test]
    fn test_seek_out_of_bounds() {
        let stream = FileInputStream::from_data(b"hello");
        assert!(stream.seek(100, SeekType::Set, None).is_err());
        assert!(stream.seek(-1, SeekType::Set, None).is_err());
    }

    #[test]
    fn test_close() {
        let stream = FileInputStream::from_data(b"hello");
        stream.close(None).unwrap();
        assert!(stream.is_closed());
        let mut buf = [0u8; 5];
        assert!(stream.read(&mut buf, None).is_err());
    }

    #[test]
    fn test_query_info() {
        let stream = FileInputStream::from_data(b"hello world");
        let info = stream.query_info("standard::*").unwrap();
        assert_eq!(info.get_size(), 11);
    }
}
