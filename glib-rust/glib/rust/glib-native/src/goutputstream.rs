//! GIO output stream matching `gio/goutputstream.h` / `gio/goutputstream.c`.
//!
//! Provides the base `OutputStream` wrapper and the concrete `MemoryOutputStream`
//! implementation for writing to in-memory buffers.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::bytes::Bytes;
use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginputstream::InputStream;
use crate::gioerror::IOErrorEnum;
use crate::prelude::*;
use alloc::sync::Arc;
use spin::Mutex;

/// Splice flags matching `GOutputStreamSpliceFlags`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum OutputStreamSpliceFlags {
    /// No flags.
    None = 0,
    /// Close the source input stream after splicing.
    CloseSource = 1 << 0,
    /// Close the target output stream after splicing.
    CloseTarget = 1 << 1,
}

/// Platform-independent trait representing an output stream implementation.
pub trait OutputStreamImpl {
    /// Returns the implementation as Any.
    fn as_any(&self) -> &dyn core::any::Any;

    /// Write data from the buffer.
    fn write(&self, buffer: &[u8], cancellable: Option<&GCancellable>) -> Result<usize, Error>;

    /// Flush the stream.
    fn flush(&self, cancellable: Option<&GCancellable>) -> Result<(), Error>;

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

/// An output stream (`GOutputStream`).
///
/// Wraps an `OutputStreamImpl` in an Arc to support cloning and reference counting.
#[derive(Clone)]
pub struct OutputStream {
    imp: Arc<dyn OutputStreamImpl + Send + Sync>,
}

impl OutputStream {
    /// Create a new `OutputStream` wrapping the implementation.
    pub fn new<O: OutputStreamImpl + Send + Sync + 'static>(imp: O) -> Self {
        Self { imp: Arc::new(imp) }
    }

    /// Write up to `buffer.len()` bytes to the stream.
    ///
    /// Mirrors `g_output_stream_write`.
    pub fn write(&self, buffer: &[u8], cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        self.imp.write(buffer, cancellable)
    }

    /// Write all data in the buffer.
    ///
    /// Mirrors `g_output_stream_write_all`.
    pub fn write_all(
        &self,
        buffer: &[u8],
        cancellable: Option<&GCancellable>,
    ) -> Result<(usize, Option<Error>), Error> {
        let mut total_written = 0;
        let mut err = None;
        while total_written < buffer.len() {
            if let Some(c) = cancellable {
                if let Err(e) = c.set_error_if_cancelled() {
                    err = Some(e);
                    break;
                }
            }
            match self.write(&buffer[total_written..], cancellable) {
                Ok(0) => break, // EOF or no space
                Ok(n) => total_written += n,
                Err(e) => {
                    err = Some(e);
                    break;
                }
            }
        }
        if total_written > 0 {
            Ok((total_written, err))
        } else if let Some(e) = err {
            Err(e)
        } else {
            Ok((0, None))
        }
    }

    /// Splice data from `source` input stream into this output stream.
    ///
    /// Mirrors `g_output_stream_splice`.
    pub fn splice(
        &self,
        source: &InputStream,
        flags: OutputStreamSpliceFlags,
        cancellable: Option<&GCancellable>,
    ) -> Result<usize, Error> {
        let mut total_spliced = 0;
        let mut buf = [0u8; 8192];
        loop {
            if let Some(c) = cancellable {
                c.set_error_if_cancelled()?;
            }
            match source.read(&mut buf, cancellable) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let mut written = 0;
                    while written < n {
                        match self.write(&buf[written..n], cancellable) {
                            Ok(0) => {
                                let err = Error::new(
                                    crate::gioerror::io_error_quark(),
                                    IOErrorEnum::Failed.to_code(),
                                    "Splice write returned 0",
                                );
                                if flags as u32 & OutputStreamSpliceFlags::CloseSource as u32 != 0 {
                                    let _ = source.close(cancellable);
                                }
                                if flags as u32 & OutputStreamSpliceFlags::CloseTarget as u32 != 0 {
                                    let _ = self.close(cancellable);
                                }
                                return Err(err);
                            }
                            Ok(w) => written += w,
                            Err(e) => {
                                if flags as u32 & OutputStreamSpliceFlags::CloseSource as u32 != 0 {
                                    let _ = source.close(cancellable);
                                }
                                if flags as u32 & OutputStreamSpliceFlags::CloseTarget as u32 != 0 {
                                    let _ = self.close(cancellable);
                                }
                                return Err(e);
                            }
                        }
                    }
                    total_spliced += n;
                }
                Err(e) => {
                    if flags as u32 & OutputStreamSpliceFlags::CloseSource as u32 != 0 {
                        let _ = source.close(cancellable);
                    }
                    if flags as u32 & OutputStreamSpliceFlags::CloseTarget as u32 != 0 {
                        let _ = self.close(cancellable);
                    }
                    return Err(e);
                }
            }
        }

        if flags as u32 & OutputStreamSpliceFlags::CloseSource as u32 != 0 {
            source.close(cancellable)?;
        }
        if flags as u32 & OutputStreamSpliceFlags::CloseTarget as u32 != 0 {
            self.close(cancellable)?;
        }

        Ok(total_spliced)
    }

    /// Flush the stream.
    ///
    /// Mirrors `g_output_stream_flush`.
    pub fn flush(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        self.imp.flush(cancellable)
    }

    /// Close the stream.
    ///
    /// Mirrors `g_output_stream_close`.
    pub fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        self.imp.close(cancellable)
    }

    /// Checks if the stream is closed.
    ///
    /// Mirrors `g_output_stream_is_closed`.
    pub fn is_closed(&self) -> bool {
        self.imp.is_closed()
    }

    /// Checks if the stream has a pending operation.
    ///
    /// Mirrors `g_output_stream_has_pending`.
    pub fn has_pending(&self) -> bool {
        self.imp.has_pending()
    }

    /// Sets the pending operation state.
    ///
    /// Mirrors `g_output_stream_set_pending`.
    pub fn set_pending(&self) -> Result<(), Error> {
        self.imp.set_pending()
    }

    /// Clears the pending operation state.
    ///
    /// Mirrors `g_output_stream_clear_pending`.
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

impl core::fmt::Debug for OutputStream {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "OutputStream")
    }
}

// ──────────────────────── MemoryOutputStream ───────────────────────────────

struct MemoryOutputStreamState {
    buffer: Vec<u8>,
    position: usize,
    valid_len: usize,
    closed: bool,
    pending: bool,
}

/// An in-memory output stream (`GMemoryOutputStream`).
pub struct MemoryOutputStream {
    state: Mutex<MemoryOutputStreamState>,
}

impl MemoryOutputStream {
    /// Create a new resizable empty `MemoryOutputStream`.
    ///
    /// Mirrors `g_memory_output_stream_new_resizable`.
    pub fn new_resizable() -> Self {
        Self {
            state: Mutex::new(MemoryOutputStreamState {
                buffer: Vec::new(),
                position: 0,
                valid_len: 0,
                closed: false,
                pending: false,
            }),
        }
    }

    /// Get a copy of the valid written data.
    ///
    /// Mirrors `g_memory_output_stream_get_data`.
    pub fn get_data(&self) -> Vec<u8> {
        let state = self.state.lock();
        state.buffer[..state.valid_len].to_vec()
    }

    /// Copy the valid written data into a destination buffer.
    pub fn copy_data(&self, dest: &mut [u8]) -> usize {
        let state = self.state.lock();
        let copy_len = state.valid_len.min(dest.len());
        dest[..copy_len].copy_from_slice(&state.buffer[..copy_len]);
        copy_len
    }

    /// Get the size of the valid written data.
    ///
    /// Mirrors `g_memory_output_stream_get_data_size`.
    pub fn get_data_size(&self) -> usize {
        self.state.lock().valid_len
    }

    /// Steal the buffer as `Bytes`, resetting the stream.
    ///
    /// Mirrors `g_memory_output_stream_steal_as_bytes`.
    pub fn steal_as_bytes(&self) -> Bytes {
        let mut state = self.state.lock();
        let buf = core::mem::take(&mut state.buffer);
        let len = state.valid_len;
        state.position = 0;
        state.valid_len = 0;
        let mut final_vec = buf;
        final_vec.truncate(len);
        Bytes::new(final_vec)
    }
}

impl Default for MemoryOutputStream {
    fn default() -> Self {
        Self::new_resizable()
    }
}

impl OutputStreamImpl for MemoryOutputStream {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn write(&self, buffer: &[u8], cancellable: Option<&GCancellable>) -> Result<usize, Error> {
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
        let pos = state.position;
        let end = pos + buffer.len();
        if end > state.buffer.len() {
            state.buffer.resize(end, 0);
        }
        state.buffer[pos..end].copy_from_slice(buffer);
        state.position = end;
        state.valid_len = state.valid_len.max(end);
        Ok(buffer.len())
    }

    fn flush(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let state = self.state.lock();
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
        Ok(())
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
        let base = match type_ {
            crate::gseekable::SeekType::Set => 0,
            crate::gseekable::SeekType::Cur => state.position as i64,
            crate::gseekable::SeekType::End => state.valid_len as i64,
        };
        let new_pos = base + offset;
        if new_pos < 0 {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "Invalid seek offset",
            ));
        }
        state.position = new_pos as usize;
        Ok(())
    }

    fn can_truncate(&self) -> bool {
        true
    }

    fn truncate(&self, offset: i64, cancellable: Option<&GCancellable>) -> Result<(), Error> {
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
        if offset < 0 {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "Invalid truncate offset",
            ));
        }
        let new_len = offset as usize;
        state.buffer.resize(new_len, 0);
        state.valid_len = new_len;
        if state.position > new_len {
            state.position = new_len;
        }
        Ok(())
    }
}

impl From<MemoryOutputStream> for OutputStream {
    fn from(s: MemoryOutputStream) -> Self {
        OutputStream::new(s)
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ginputstream::MemoryInputStream;

    #[test]
    fn test_memory_output_stream_write() {
        let stream = OutputStream::from(MemoryOutputStream::new_resizable());
        let n = stream.write(b"hello", None).unwrap();
        assert_eq!(n, 5);

        let n2 = stream.write(b" world", None).unwrap();
        assert_eq!(n2, 6);
    }

    #[test]
    fn test_memory_output_stream_write_all() {
        let stream = OutputStream::from(MemoryOutputStream::new_resizable());
        let (n, err) = stream.write_all(b"write all data", None).unwrap();
        assert_eq!(n, 14);
        assert!(err.is_none());
    }

    #[test]
    fn test_memory_output_stream_get_data() {
        let mem_stream = MemoryOutputStream::new_resizable();
        let stream = OutputStream::from(mem_stream);
        stream.write(b"some data", None).unwrap();
        assert_eq!(stream.is_closed(), false);

        let underlying = stream.downcast_ref::<MemoryOutputStream>().unwrap();
        assert_eq!(underlying.get_data(), b"some data");
        assert_eq!(underlying.get_data_size(), 9);
    }

    #[test]
    fn test_memory_output_stream_steal_as_bytes() {
        let mem_stream = MemoryOutputStream::new_resizable();
        let stream = OutputStream::from(mem_stream);
        stream.write(b"steal me", None).unwrap();

        let underlying = stream.downcast_ref::<MemoryOutputStream>().unwrap();
        let bytes = underlying.steal_as_bytes();
        assert_eq!(bytes.as_ref(), b"steal me");
        assert_eq!(underlying.get_data_size(), 0);
    }

    #[test]
    fn test_memory_output_stream_close() {
        let stream = OutputStream::from(MemoryOutputStream::new_resizable());
        assert!(!stream.is_closed());
        stream.close(None).unwrap();
        assert!(stream.is_closed());
        assert_eq!(
            stream.write(b"fail", None).unwrap_err().code(),
            IOErrorEnum::Closed.to_code()
        );
    }

    #[test]
    fn test_memory_output_stream_pending() {
        let stream = OutputStream::from(MemoryOutputStream::new_resizable());
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
    fn test_memory_output_stream_cancellation() {
        let stream = OutputStream::from(MemoryOutputStream::new_resizable());
        let cancellable = GCancellable::new();
        cancellable.cancel();

        assert_eq!(
            stream
                .write(b"fail", Some(&cancellable))
                .unwrap_err()
                .code(),
            IOErrorEnum::Cancelled.to_code()
        );
    }

    #[test]
    fn test_stream_splice() {
        let source_bytes = Bytes::from_static(b"splice content");
        let source = InputStream::from(MemoryInputStream::new_from_bytes(source_bytes));
        let dest_stream = MemoryOutputStream::new_resizable();
        let dest = OutputStream::from(dest_stream);

        let spliced = dest
            .splice(&source, OutputStreamSpliceFlags::None, None)
            .unwrap();
        assert_eq!(spliced, 14);

        let underlying = dest.downcast_ref::<MemoryOutputStream>().unwrap();
        assert_eq!(underlying.get_data(), b"splice content");
    }

    #[test]
    fn test_memory_output_stream_seek() {
        use crate::gseekable::SeekType;
        let mem = MemoryOutputStream::new_resizable();
        let stream = OutputStream::from(mem);
        assert!(stream.can_seek());
        assert!(stream.can_truncate());

        stream.write(b"0123456789", None).unwrap();

        stream.seek(4, SeekType::Set, None).unwrap();
        stream.write(b"abc", None).unwrap();

        let underlying = stream.downcast_ref::<MemoryOutputStream>().unwrap();
        assert_eq!(underlying.get_data(), b"0123abc789");

        stream.seek(-1, SeekType::Cur, None).unwrap();
        stream.write(b"xyz", None).unwrap();
        assert_eq!(underlying.get_data(), b"0123abxyz9");

        stream.seek(0, SeekType::End, None).unwrap();
        stream.write(b"!", None).unwrap();
        assert_eq!(underlying.get_data(), b"0123abxyz9!");

        // Truncate
        stream.truncate(5, None).unwrap();
        assert_eq!(underlying.get_data(), b"0123a");

        assert!(stream.seek(-1, SeekType::Set, None).is_err());
        assert!(stream.truncate(-1, None).is_err());
    }
}

// Helper trait to allow downcasting implementation back to concrete types for tests.
trait Downcast {
    fn downcast_ref<T: 'static>(&self) -> Option<&T>;
}

impl<I: OutputStreamImpl + Send + Sync + 'static> Downcast for I {
    fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        let any = self as &dyn core::any::Any;
        any.downcast_ref::<T>()
    }
}

impl OutputStream {
    /// Downcasts the stream implementation to a concrete type.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.imp.as_any().downcast_ref::<T>()
    }
}
