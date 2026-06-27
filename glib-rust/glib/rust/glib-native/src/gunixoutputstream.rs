//! GUnixOutputStream matching `gio/gunixoutputstream.h` / `gio/gunixoutputstream.c`.
//!
//! A `GOutputStream` wrapping a Unix file descriptor. On bare-metal `no_std`
//! targets fd I/O is simulated with an internal transmit log keyed by `fd`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::gioerror::{io_error_quark, IOErrorEnum};
use crate::goutputstream::{OutputStream, OutputStreamImpl};
use alloc::vec::Vec;
use spin::Mutex;

/// Internal state for [`UnixOutputStream`].
struct UnixOutputStreamState {
    fd: i32,
    tx_log: Vec<u8>,
    stream_closed: bool,
    close_fd: bool,
    pending: bool,
}

/// A Unix fd-backed output stream (`GUnixOutputStream`).
pub struct UnixOutputStream {
    state: Mutex<UnixOutputStreamState>,
}

impl UnixOutputStream {
    /// Creates a new `UnixOutputStream` for `fd`.
    ///
    /// Mirrors `g_unix_output_stream_new`.
    pub fn new(fd: i32) -> Self {
        Self::new_with_close_fd(fd, false)
    }

    /// Creates a stream with explicit `close_fd` behaviour.
    pub fn new_with_close_fd(fd: i32, close_fd: bool) -> Self {
        Self {
            state: Mutex::new(UnixOutputStreamState {
                fd,
                tx_log: Vec::new(),
                stream_closed: false,
                close_fd,
                pending: false,
            }),
        }
    }

    /// Returns the wrapped file descriptor.
    ///
    /// Mirrors `g_unix_output_stream_get_fd`.
    pub fn get_fd(&self) -> i32 {
        self.state.lock().fd
    }

    /// Sets whether the fd should be closed when the stream is closed.
    ///
    /// Mirrors `g_unix_output_stream_set_close_fd`.
    pub fn set_close_fd(&self, close_fd: bool) {
        self.state.lock().close_fd = close_fd;
    }

    /// Returns whether the fd will be closed with the stream.
    ///
    /// Mirrors `g_unix_output_stream_get_close_fd`.
    pub fn get_close_fd(&self) -> bool {
        self.state.lock().close_fd
    }

    /// Returns a copy of bytes written through this stream (simulation helper).
    pub fn get_tx_data(&self) -> Vec<u8> {
        self.state.lock().tx_log.clone()
    }

    /// Write up to `buffer.len()` bytes to the stream.
    ///
    /// Mirrors `g_output_stream_write`.
    pub fn write(&self, buffer: &[u8], cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        OutputStreamImpl::write(self, buffer, cancellable)
    }

    /// Write all data in `buffer`.
    ///
    /// Mirrors `g_output_stream_write_all`.
    pub fn write_all(
        &self,
        buffer: &[u8],
        cancellable: Option<&GCancellable>,
    ) -> Result<(usize, Option<Error>), Error> {
        OutputStream::from(self.clone()).write_all(buffer, cancellable)
    }

    /// Flush the stream (no-op for simulated fd output; validates state).
    ///
    /// Mirrors `g_output_stream_flush`.
    pub fn flush(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        OutputStreamImpl::flush(self, cancellable)
    }

    /// Close the stream and optionally the fd.
    ///
    /// Mirrors `g_output_stream_close`.
    pub fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        OutputStreamImpl::close(self, cancellable)
    }

    /// Returns whether the stream is closed.
    ///
    /// Mirrors `g_output_stream_is_closed`.
    pub fn is_closed(&self) -> bool {
        OutputStreamImpl::is_closed(self)
    }
}

impl Clone for UnixOutputStream {
    fn clone(&self) -> Self {
        let st = self.state.lock();
        Self {
            state: Mutex::new(UnixOutputStreamState {
                fd: st.fd,
                tx_log: st.tx_log.clone(),
                stream_closed: st.stream_closed,
                close_fd: st.close_fd,
                pending: st.pending,
            }),
        }
    }
}

impl OutputStreamImpl for UnixOutputStream {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn write(&self, buffer: &[u8], cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        let mut st = self.state.lock();
        if st.stream_closed {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        if buffer.is_empty() {
            return Ok(0);
        }
        if st.fd < 0 {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "File descriptor is closed",
            ));
        }
        st.tx_log.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let st = self.state.lock();
        if st.stream_closed {
            return Err(Error::new(
                io_error_quark(),
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
        let mut st = self.state.lock();
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        st.stream_closed = true;
        if st.close_fd {
            st.fd = -1;
        }
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.state.lock().stream_closed
    }

    fn has_pending(&self) -> bool {
        self.state.lock().pending
    }

    fn set_pending(&self) -> Result<(), Error> {
        let mut st = self.state.lock();
        if st.stream_closed {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        if st.pending {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Pending.to_code(),
                "Stream has pending operation",
            ));
        }
        st.pending = true;
        Ok(())
    }

    fn clear_pending(&self) {
        self.state.lock().pending = false;
    }
}

impl From<UnixOutputStream> for OutputStream {
    fn from(s: UnixOutputStream) -> Self {
        OutputStream::new(s)
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_and_get_fd() {
        let s = UnixOutputStream::new_with_close_fd(4, true);
        assert_eq!(s.get_fd(), 4);
        assert!(s.get_close_fd());
    }

    #[test]
    fn test_set_close_fd() {
        let s = UnixOutputStream::new(1);
        s.set_close_fd(true);
        assert!(s.get_close_fd());
    }

    #[test]
    fn test_write() {
        let s = UnixOutputStream::new(1);
        assert_eq!(s.write(b"hello", None).unwrap(), 5);
        assert_eq!(s.get_tx_data(), b"hello".to_vec());
    }

    #[test]
    fn test_write_all() {
        let s = UnixOutputStream::new(1);
        let (n, err) = s.write_all(b"abc", None).unwrap();
        assert_eq!(n, 3);
        assert!(err.is_none());
    }

    #[test]
    fn test_close_with_fd() {
        let s = UnixOutputStream::new_with_close_fd(9, true);
        s.close(None).unwrap();
        assert!(s.is_closed());
        assert_eq!(s.get_fd(), -1);
    }

    #[test]
    fn test_closed_write_fails() {
        let s = UnixOutputStream::new(0);
        s.close(None).unwrap();
        assert_eq!(
            s.write(b"x", None).unwrap_err().code(),
            IOErrorEnum::Closed.to_code()
        );
    }
}
