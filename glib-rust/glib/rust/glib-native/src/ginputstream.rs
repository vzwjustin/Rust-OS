//! GIO input stream matching `gio/ginputstream.h` / `gio/ginputstream.c`.
//!
//! Provides the base `InputStream` wrapper and the concrete `MemoryInputStream`
//! implementation for reading from in-memory buffers.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::bytes::Bytes;
use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::gioerror::IOErrorEnum;
use crate::prelude::*;
use alloc::sync::Arc;
use spin::Mutex;

/// Platform-independent trait representing an input stream implementation.
pub trait InputStreamImpl {
    /// Returns the implementation as Any.
    fn as_any(&self) -> &dyn core::any::Any;

    /// Read data into the buffer.
    fn read(&self, buffer: &mut [u8], cancellable: Option<&GCancellable>) -> Result<usize, Error>;

    /// Skip data in the stream.
    fn skip(&self, count: usize, cancellable: Option<&GCancellable>) -> Result<usize, Error>;

    /// Close the stream.
    fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error>;

    /// Returns whether the stream is closed.
    fn is_closed(&self) -> bool;

    /// Returns whether the stream has a pending operation.
    fn has_pending(&self) -> bool;

    /// Marks the stream as having a pending operation.
    fn set_pending(&self) -> Result<(), Error>;

    /// Clears the pending operation mark.
    fn clear_pending(&self);

    /// Checks if the stream supports seeking.
    fn can_seek(&self) -> bool {
        false
    }

    /// Seeks the stream.
    fn seek(
        &self,
        _offset: i64,
        _type_: crate::gseekable::SeekType,
        _cancellable: Option<&GCancellable>,
    ) -> Result<(), Error> {
        Err(Error::new(
            crate::gioerror::io_error_quark(),
            IOErrorEnum::NotSupported.to_code(),
            "Seek not supported",
        ))
    }

    /// Checks if the stream supports truncating.
    fn can_truncate(&self) -> bool {
        false
    }

    /// Truncates the stream.
    fn truncate(&self, _offset: i64, _cancellable: Option<&GCancellable>) -> Result<(), Error> {
        Err(Error::new(
            crate::gioerror::io_error_quark(),
            IOErrorEnum::NotSupported.to_code(),
            "Truncation not supported",
        ))
    }
}

/// An input stream (`GInputStream`).
///
/// Wraps an `InputStreamImpl` in an Arc to support cloning and reference counting.
#[derive(Clone)]
pub struct InputStream {
    imp: Arc<dyn InputStreamImpl + Send + Sync>,
}

impl InputStream {
    /// Create a new `InputStream` wrapping the implementation.
    pub fn new<I: InputStreamImpl + Send + Sync + 'static>(imp: I) -> Self {
        Self { imp: Arc::new(imp) }
    }

    /// Read up to `buffer.len()` bytes from the stream.
    ///
    /// Mirrors `g_input_stream_read`.
    pub fn read(
        &self,
        buffer: &mut [u8],
        cancellable: Option<&GCancellable>,
    ) -> Result<usize, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        self.imp.read(buffer, cancellable)
    }

    /// Read all data up to `buffer.len()` bytes.
    ///
    /// Mirrors `g_input_stream_read_all`.
    pub fn read_all(
        &self,
        buffer: &mut [u8],
        cancellable: Option<&GCancellable>,
    ) -> Result<(usize, Option<Error>), Error> {
        let mut total_read = 0;
        let mut err = None;
        while total_read < buffer.len() {
            if let Some(c) = cancellable {
                if let Err(e) = c.set_error_if_cancelled() {
                    err = Some(e);
                    break;
                }
            }
            match self.read(&mut buffer[total_read..], cancellable) {
                Ok(0) => break, // EOF
                Ok(n) => total_read += n,
                Err(e) => {
                    err = Some(e);
                    break;
                }
            }
        }
        if total_read > 0 {
            Ok((total_read, err))
        } else if let Some(e) = err {
            Err(e)
        } else {
            Ok((0, None))
        }
    }

    /// Skip `count` bytes from the stream.
    ///
    /// Mirrors `g_input_stream_skip`.
    pub fn skip(&self, count: usize, cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        self.imp.skip(count, cancellable)
    }

    /// Close the stream.
    ///
    /// Mirrors `g_input_stream_close`.
    pub fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        self.imp.close(cancellable)
    }

    /// Checks if the stream is closed.
    ///
    /// Mirrors `g_input_stream_is_closed`.
    pub fn is_closed(&self) -> bool {
        self.imp.is_closed()
    }

    /// Checks if the stream has a pending operation.
    ///
    /// Mirrors `g_input_stream_has_pending`.
    pub fn has_pending(&self) -> bool {
        self.imp.has_pending()
    }

    /// Sets the pending operation state.
    ///
    /// Mirrors `g_input_stream_set_pending`.
    pub fn set_pending(&self) -> Result<(), Error> {
        self.imp.set_pending()
    }

    /// Clears the pending operation state.
    ///
    /// Mirrors `g_input_stream_clear_pending`.
    pub fn clear_pending(&self) {
        self.imp.clear_pending()
    }

    /// Checks if the stream supports seeking.
    pub fn can_seek(&self) -> bool {
        self.imp.can_seek()
    }

    /// Seeks the stream.
    pub fn seek(
        &self,
        offset: i64,
        type_: crate::gseekable::SeekType,
        cancellable: Option<&GCancellable>,
    ) -> Result<(), Error> {
        self.imp.seek(offset, type_, cancellable)
    }

    /// Checks if the stream supports truncating.
    pub fn can_truncate(&self) -> bool {
        self.imp.can_truncate()
    }

    /// Truncates the stream.
    pub fn truncate(&self, offset: i64, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        self.imp.truncate(offset, cancellable)
    }
}

impl core::fmt::Debug for InputStream {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "InputStream")
    }
}

// ──────────────────────── MemoryInputStream ───────────────────────────────

struct MemoryInputStreamState {
    bytes: Bytes,
    position: usize,
    closed: bool,
    pending: bool,
}

/// An in-memory input stream (`GMemoryInputStream`).
pub struct MemoryInputStream {
    state: Mutex<MemoryInputStreamState>,
}

impl MemoryInputStream {
    /// Create a new empty `MemoryInputStream`.
    ///
    /// Mirrors `g_memory_input_stream_new`.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(MemoryInputStreamState {
                bytes: Bytes::new(&[]),
                position: 0,
                closed: false,
                pending: false,
            }),
        }
    }

    /// Create a new `MemoryInputStream` initialized with `bytes`.
    ///
    /// Mirrors `g_memory_input_stream_new_from_bytes`.
    pub fn new_from_bytes(bytes: Bytes) -> Self {
        Self {
            state: Mutex::new(MemoryInputStreamState {
                bytes,
                position: 0,
                closed: false,
                pending: false,
            }),
        }
    }

    /// Create a new `MemoryInputStream` initialized with `data`.
    ///
    /// Mirrors `g_memory_input_stream_new_from_data`.
    pub fn new_from_data(data: &[u8]) -> Self {
        Self::new_from_bytes(Bytes::new(data))
    }

    /// Appends `bytes` to the end of the data stream.
    ///
    /// Mirrors `g_memory_input_stream_add_bytes`.
    pub fn add_bytes(&self, bytes: Bytes) {
        let mut state = self.state.lock();
        if state.closed {
            return;
        }
        let mut new_vec = Vec::new();
        new_vec.extend_from_slice(state.bytes.as_ref());
        new_vec.extend_from_slice(bytes.as_ref());
        state.bytes = Bytes::new(new_vec);
    }
}

impl Default for MemoryInputStream {
    fn default() -> Self {
        Self::new()
    }
}

impl InputStreamImpl for MemoryInputStream {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn read(&self, buffer: &mut [u8], cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        let mut state = self.state.lock();
        if state.closed {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        let data = state.bytes.as_ref();
        if state.position >= data.len() {
            return Ok(0); // EOF
        }
        let available = data.len() - state.position;
        let to_read = available.min(buffer.len());
        buffer[..to_read].copy_from_slice(&data[state.position..state.position + to_read]);
        state.position += to_read;
        Ok(to_read)
    }

    fn skip(&self, count: usize, cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        let mut state = self.state.lock();
        if state.closed {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        let data = state.bytes.as_ref();
        if state.position >= data.len() {
            return Ok(0);
        }
        let available = data.len() - state.position;
        let to_skip = available.min(count);
        state.position += to_skip;
        Ok(to_skip)
    }

    fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let mut state = self.state.lock();
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        state.closed = true;
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.state.lock().closed
    }

    fn has_pending(&self) -> bool {
        self.state.lock().pending
    }

    fn set_pending(&self) -> Result<(), Error> {
        let mut state = self.state.lock();
        if state.closed {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        if state.pending {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::Pending.to_code(),
                "Stream has pending operation",
            ));
        }
        state.pending = true;
        Ok(())
    }

    fn clear_pending(&self) {
        self.state.lock().pending = false;
    }

    fn can_seek(&self) -> bool {
        true
    }

    fn seek(
        &self,
        offset: i64,
        type_: crate::gseekable::SeekType,
        cancellable: Option<&GCancellable>,
    ) -> Result<(), Error> {
        let mut state = self.state.lock();
        if state.closed {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        let data_len = state.bytes.len() as i64;
        let new_pos = match type_ {
            crate::gseekable::SeekType::Set => offset,
            crate::gseekable::SeekType::Cur => (state.position as i64) + offset,
            crate::gseekable::SeekType::End => data_len + offset,
        };
        if new_pos < 0 || new_pos > data_len {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "Invalid seek offset",
            ));
        }
        state.position = new_pos as usize;
        Ok(())
    }
}

impl From<MemoryInputStream> for InputStream {
    fn from(s: MemoryInputStream) -> Self {
        InputStream::new(s)
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_input_stream_read() {
        let bytes = Bytes::from_static(b"hello world");
        let stream = InputStream::from(MemoryInputStream::new_from_bytes(bytes));
        let mut buf = [0u8; 5];
        let n = stream.read(&mut buf, None).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");

        let mut buf2 = [0u8; 10];
        let n2 = stream.read(&mut buf2, None).unwrap();
        assert_eq!(n2, 6);
        assert_eq!(&buf2[..6], b" world");
    }

    #[test]
    fn test_memory_input_stream_read_all() {
        let bytes = Bytes::from_static(b"test data");
        let stream = InputStream::from(MemoryInputStream::new_from_bytes(bytes));
        let mut buf = [0u8; 20];
        let (n, err) = stream.read_all(&mut buf, None).unwrap();
        assert_eq!(n, 9);
        assert!(err.is_none());
        assert_eq!(&buf[..9], b"test data");
    }

    #[test]
    fn test_memory_input_stream_skip() {
        let bytes = Bytes::from_static(b"abcdefgh");
        let stream = InputStream::from(MemoryInputStream::new_from_bytes(bytes));
        let skipped = stream.skip(3, None).unwrap();
        assert_eq!(skipped, 3);
        let mut buf = [0u8; 2];
        let n = stream.read(&mut buf, None).unwrap();
        assert_eq!(n, 2);
        assert_eq!(&buf, b"de");
    }

    #[test]
    fn test_memory_input_stream_add_bytes() {
        let mem_stream = MemoryInputStream::new();
        mem_stream.add_bytes(Bytes::from_static(b"hello"));
        mem_stream.add_bytes(Bytes::from_static(b" world"));
        let stream = InputStream::from(mem_stream);
        let mut buf = [0u8; 11];
        let n = stream.read(&mut buf, None).unwrap();
        assert_eq!(n, 11);
        assert_eq!(&buf, b"hello world");
    }

    #[test]
    fn test_memory_input_stream_close() {
        let stream = InputStream::from(MemoryInputStream::new());
        assert!(!stream.is_closed());
        stream.close(None).unwrap();
        assert!(stream.is_closed());
        let mut buf = [0u8; 4];
        assert_eq!(
            stream.read(&mut buf, None).unwrap_err().code(),
            IOErrorEnum::Closed.to_code()
        );
    }

    #[test]
    fn test_memory_input_stream_pending() {
        let stream = InputStream::from(MemoryInputStream::new());
        assert!(!stream.has_pending());
        stream.set_pending().unwrap();
        assert!(stream.has_pending());
        assert_eq!(
            stream.set_pending().unwrap_err().code(),
            IOErrorEnum::Pending.to_code()
        );
        stream.clear_pending();
        assert!(!stream.has_pending());
    }

    #[test]
    fn test_memory_input_stream_cancellation() {
        let bytes = Bytes::from_static(b"hello");
        let stream = InputStream::from(MemoryInputStream::new_from_bytes(bytes));
        let cancellable = GCancellable::new();
        cancellable.cancel();

        let mut buf = [0u8; 5];
        assert_eq!(
            stream
                .read(&mut buf, Some(&cancellable))
                .unwrap_err()
                .code(),
            IOErrorEnum::Cancelled.to_code()
        );
    }

    #[test]
    fn test_memory_input_stream_seek() {
        use crate::gseekable::SeekType;
        let bytes = Bytes::from_static(b"0123456789");
        let stream = InputStream::from(MemoryInputStream::new_from_bytes(bytes));
        assert!(stream.can_seek());
        assert!(!stream.can_truncate());

        stream.seek(4, SeekType::Set, None).unwrap();
        let mut buf = [0u8; 3];
        stream.read(&mut buf, None).unwrap();
        assert_eq!(&buf, b"456");

        stream.seek(-2, SeekType::Cur, None).unwrap();
        let mut buf2 = [0u8; 2];
        stream.read(&mut buf2, None).unwrap();
        assert_eq!(&buf2, b"56");

        stream.seek(-3, SeekType::End, None).unwrap();
        let mut buf3 = [0u8; 3];
        stream.read(&mut buf3, None).unwrap();
        assert_eq!(&buf3, b"789");

        assert!(stream.seek(-11, SeekType::Set, None).is_err());
        assert!(stream.seek(1, SeekType::End, None).is_err());
    }
}

// Helper trait to allow downcasting implementation back to concrete types for tests.
trait Downcast {
    fn downcast_ref<T: 'static>(&self) -> Option<&T>;
}

impl<I: InputStreamImpl + Send + Sync + 'static> Downcast for I {
    fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        let any = self as &dyn core::any::Any;
        any.downcast_ref::<T>()
    }
}

impl InputStream {
    /// Downcasts the stream implementation to a concrete type.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.imp.as_any().downcast_ref::<T>()
    }
}
