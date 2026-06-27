//! GUnixInputStream matching `gio/gunixinputstream.h` / `gio/gunixinputstream.c`.
//!
//! A `GInputStream` wrapping a Unix file descriptor. On bare-metal `no_std`
//! targets fd I/O is simulated with an internal byte buffer keyed by `fd`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginputstream::{InputStream, InputStreamImpl};
use crate::gioerror::{io_error_quark, IOErrorEnum};
use alloc::vec::Vec;
use spin::Mutex;

/// Internal state for [`UnixInputStream`].
struct UnixInputStreamState {
    fd: i32,
    buffer: Vec<u8>,
    stream_closed: bool,
    close_fd: bool,
    pending: bool,
}

/// A Unix fd-backed input stream (`GUnixInputStream`).
pub struct UnixInputStream {
    state: Mutex<UnixInputStreamState>,
}

impl UnixInputStream {
    /// Creates a new `UnixInputStream` for `fd`.
    ///
    /// When `close_fd` is `true`, closing the stream marks the fd as closed
    /// in the simulated environment.
    ///
    /// Mirrors `g_unix_input_stream_new`.
    pub fn new(fd: i32) -> Self {
        Self::new_with_close_fd(fd, false)
    }

    /// Creates a stream with explicit `close_fd` behaviour.
    pub fn new_with_close_fd(fd: i32, close_fd: bool) -> Self {
        Self {
            state: Mutex::new(UnixInputStreamState {
                fd,
                buffer: Vec::new(),
                stream_closed: false,
                close_fd,
                pending: false,
            }),
        }
    }

    /// Returns the wrapped file descriptor.
    ///
    /// Mirrors `g_unix_input_stream_get_fd`.
    pub fn get_fd(&self) -> i32 {
        self.state.lock().fd
    }

    /// Sets whether the fd should be closed when the stream is closed.
    ///
    /// Mirrors `g_unix_input_stream_set_close_fd`.
    pub fn set_close_fd(&self, close_fd: bool) {
        self.state.lock().close_fd = close_fd;
    }

    /// Returns whether the fd will be closed with the stream.
    ///
    /// Mirrors `g_unix_input_stream_get_close_fd`.
    pub fn get_close_fd(&self) -> bool {
        self.state.lock().close_fd
    }

    /// Injects bytes into the simulated read buffer (test / no_std helper).
    pub fn inject(&self, data: &[u8]) {
        let mut st = self.state.lock();
        if st.stream_closed {
            return;
        }
        st.buffer.extend_from_slice(data);
    }

    /// Returns buffered bytes not yet consumed by [`read`](Self::read).
    pub fn buffered_available(&self) -> usize {
        self.state.lock().buffer.len()
    }

    /// Read up to `buffer.len()` bytes from the stream.
    ///
    /// Mirrors `g_input_stream_read`.
    pub fn read(
        &self,
        buffer: &mut [u8],
        cancellable: Option<&GCancellable>,
    ) -> Result<usize, Error> {
        InputStreamImpl::read(self, buffer, cancellable)
    }

    /// Read all data up to `buffer.len()` bytes.
    ///
    /// Mirrors `g_input_stream_read_all`.
    pub fn read_all(
        &self,
        buffer: &mut [u8],
        cancellable: Option<&GCancellable>,
    ) -> Result<(usize, Option<Error>), Error> {
        InputStream::from(self.clone()).read_all(buffer, cancellable)
    }

    /// Close the stream and optionally the fd.
    ///
    /// Mirrors `g_input_stream_close`.
    pub fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        InputStreamImpl::close(self, cancellable)
    }

    /// Returns whether the stream is closed.
    ///
    /// Mirrors `g_input_stream_is_closed`.
    pub fn is_closed(&self) -> bool {
        InputStreamImpl::is_closed(self)
    }
}

impl Clone for UnixInputStream {
    fn clone(&self) -> Self {
        let st = self.state.lock();
        Self {
            state: Mutex::new(UnixInputStreamState {
                fd: st.fd,
                buffer: st.buffer.clone(),
                stream_closed: st.stream_closed,
                close_fd: st.close_fd,
                pending: st.pending,
            }),
        }
    }
}

impl InputStreamImpl for UnixInputStream {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn read(&self, buffer: &mut [u8], cancellable: Option<&GCancellable>) -> Result<usize, Error> {
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
        if st.buffer.is_empty() {
            return Ok(0);
        }
        let n = buffer.len().min(st.buffer.len());
        buffer[..n].copy_from_slice(&st.buffer[..n]);
        st.buffer.drain(..n);
        Ok(n)
    }

    fn skip(&self, count: usize, cancellable: Option<&GCancellable>) -> Result<usize, Error> {
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
        let n = count.min(st.buffer.len());
        st.buffer.drain(..n);
        Ok(n)
    }

    fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let mut st = self.state.lock();
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        st.stream_closed = true;
        st.buffer.clear();
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

impl From<UnixInputStream> for InputStream {
    fn from(s: UnixInputStream) -> Self {
        InputStream::new(s)
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_and_get_fd() {
        let s = UnixInputStream::new_with_close_fd(3, true);
        assert_eq!(s.get_fd(), 3);
        assert!(s.get_close_fd());
        assert!(!s.is_closed());
    }

    #[test]
    fn test_set_close_fd() {
        let s = UnixInputStream::new(5);
        assert!(!s.get_close_fd());
        s.set_close_fd(true);
        assert!(s.get_close_fd());
    }

    #[test]
    fn test_read() {
        let s = UnixInputStream::new(0);
        s.inject(b"data");
        let mut buf = [0u8; 2];
        assert_eq!(s.read(&mut buf, None).unwrap(), 2);
        assert_eq!(&buf, b"da");
    }

    #[test]
    fn test_read_all() {
        let s = UnixInputStream::new(0);
        s.inject(b"hello");
        let mut buf = [0u8; 10];
        let (n, err) = s.read_all(&mut buf, None).unwrap();
        assert_eq!(n, 5);
        assert!(err.is_none());
    }

    #[test]
    fn test_close_with_fd() {
        let s = UnixInputStream::new_with_close_fd(7, true);
        s.close(None).unwrap();
        assert!(s.is_closed());
        assert_eq!(s.get_fd(), -1);
    }

    #[test]
    fn test_close_without_fd() {
        let s = UnixInputStream::new_with_close_fd(7, false);
        s.close(None).unwrap();
        assert_eq!(s.get_fd(), 7);
    }

    #[test]
    fn test_closed_read_fails() {
        let s = UnixInputStream::new(0);
        s.close(None).unwrap();
        let mut buf = [0u8; 1];
        assert_eq!(
            s.read(&mut buf, None).unwrap_err().code(),
            IOErrorEnum::Closed.to_code()
        );
    }
}
