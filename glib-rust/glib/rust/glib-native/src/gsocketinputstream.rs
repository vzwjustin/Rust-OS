//! GSocketInputStream matching `gio/gsocketinputstream.h` / `gio/gsocketinputstream.c`.
//!
//! A `GInputStream` backed by a `GSocket`. On bare-metal `no_std` targets the
//! underlying socket is modelled via the [`Socket`] trait (typically
//! [`MockSocket`]); an optional internal read buffer supplements `receive()`
//! for test injection.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginputstream::{InputStream, InputStreamImpl};
use crate::gioerror::{io_error_quark, IOErrorEnum};
use crate::gsocket::{MockSocket, Socket};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

/// Internal state for [`SocketInputStream`].
struct SocketInputStreamState {
    /// Extra bytes available before calling into the socket `receive()`.
    read_buffer: Vec<u8>,
    stream_closed: bool,
    pending: bool,
}

/// A socket-backed input stream (`GSocketInputStream`).
pub struct SocketInputStream {
    socket: Arc<dyn Socket + Send + Sync>,
    state: Mutex<SocketInputStreamState>,
}

impl SocketInputStream {
    /// Creates a new `SocketInputStream` wrapping `socket`.
    ///
    /// Mirrors `g_socket_input_stream_new`.
    pub fn new<S: Socket + Send + Sync + 'static>(socket: S) -> Self {
        Self::new_arc(Arc::new(socket))
    }

    /// Creates a new `SocketInputStream` from an existing `Arc` socket.
    pub fn new_arc(socket: Arc<dyn Socket + Send + Sync>) -> Self {
        Self {
            socket,
            state: Mutex::new(SocketInputStreamState {
                read_buffer: Vec::new(),
                stream_closed: false,
                pending: false,
            }),
        }
    }

    /// Creates a stream backed by a fresh connected [`MockSocket`].
    pub fn new_mock() -> Self {
        Self::new(MockSocket::new_stream())
    }

    /// Returns a clone of the underlying socket.
    ///
    /// Mirrors `g_socket_input_stream_get_socket`.
    pub fn get_socket(&self) -> Arc<dyn Socket + Send + Sync> {
        Arc::clone(&self.socket)
    }

    /// Injects bytes into the internal read buffer (test / simulation helper).
    ///
    /// Data is returned by [`read`](Self::read) before the socket `receive()`
    /// path is consulted.
    pub fn inject(&self, data: &[u8]) {
        let mut st = self.state.lock();
        if st.stream_closed {
            return;
        }
        st.read_buffer.extend_from_slice(data);
    }

    /// Returns the number of bytes waiting in the internal read buffer.
    pub fn buffered_available(&self) -> usize {
        self.state.lock().read_buffer.len()
    }

    /// Read up to `buffer.len()` bytes from the stream.
    ///
    /// Mirrors `g_input_stream_read` on a `GSocketInputStream`.
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
        let mut total_read = 0;
        let mut err = None;
        while total_read < buffer.len() {
            match self.read(&mut buffer[total_read..], cancellable) {
                Ok(0) => break,
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

impl Clone for SocketInputStream {
    fn clone(&self) -> Self {
        Self {
            socket: Arc::clone(&self.socket),
            state: Mutex::new(SocketInputStreamState {
                read_buffer: self.state.lock().read_buffer.clone(),
                stream_closed: self.state.lock().stream_closed,
                pending: self.state.lock().pending,
            }),
        }
    }
}

impl InputStreamImpl for SocketInputStream {
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

        let mut total = 0usize;
        if !st.read_buffer.is_empty() {
            let n = buffer.len().min(st.read_buffer.len());
            buffer[..n].copy_from_slice(&st.read_buffer[..n]);
            st.read_buffer.drain(..n);
            total += n;
            if total == buffer.len() {
                return Ok(total);
            }
        }

        if self.socket.is_closed() {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Underlying socket is closed",
            ));
        }

        let n = self.socket.receive(&mut buffer[total..], cancellable)?;
        total += n;
        Ok(total)
    }

    fn skip(&self, count: usize, cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        let mut skip_buf = [0u8; 4096];
        let mut remaining = count;
        let mut skipped = 0usize;
        while remaining > 0 {
            let chunk = remaining.min(skip_buf.len());
            let n = self.read(&mut skip_buf[..chunk], cancellable)?;
            if n == 0 {
                break;
            }
            skipped += n;
            remaining -= n;
        }
        Ok(skipped)
    }

    fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let mut st = self.state.lock();
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        st.stream_closed = true;
        st.read_buffer.clear();
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

impl From<SocketInputStream> for InputStream {
    fn from(s: SocketInputStream) -> Self {
        InputStream::new(s)
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gsocket::MockSocket;

    #[test]
    fn test_read_from_injected_buffer() {
        let stream = SocketInputStream::new_mock();
        stream.inject(b"hello");
        let mut buf = [0u8; 3];
        assert_eq!(stream.read(&mut buf, None).unwrap(), 3);
        assert_eq!(&buf, b"hel");
        assert_eq!(stream.buffered_available(), 2);
    }

    #[test]
    fn test_read_from_socket_receive() {
        let mock = MockSocket::new_stream();
        mock.inject(b"socket data");
        let stream = SocketInputStream::new(mock);
        let mut buf = [0u8; 11];
        assert_eq!(stream.read(&mut buf, None).unwrap(), 11);
        assert_eq!(&buf, b"socket data");
    }

    #[test]
    fn test_read_all() {
        let stream = SocketInputStream::new_mock();
        stream.inject(b"abcdef");
        let mut buf = [0u8; 10];
        let (n, err) = stream.read_all(&mut buf, None).unwrap();
        assert_eq!(n, 6);
        assert!(err.is_none());
        assert_eq!(&buf[..6], b"abcdef");
    }

    #[test]
    fn test_get_socket() {
        let mock = MockSocket::new_stream();
        let stream = SocketInputStream::new(mock);
        assert!(!stream.get_socket().is_closed());
    }

    #[test]
    fn test_close() {
        let stream = SocketInputStream::new_mock();
        assert!(!stream.is_closed());
        stream.close(None).unwrap();
        assert!(stream.is_closed());
        let mut buf = [0u8; 1];
        assert_eq!(
            stream.read(&mut buf, None).unwrap_err().code(),
            IOErrorEnum::Closed.to_code()
        );
    }

    #[test]
    fn test_input_stream_wrapper() {
        let socket_stream = SocketInputStream::new_mock();
        socket_stream.inject(b"wrap");
        let stream = InputStream::from(socket_stream);
        let mut buf = [0u8; 4];
        assert_eq!(stream.read(&mut buf, None).unwrap(), 4);
        assert_eq!(&buf, b"wrap");
    }

    #[test]
    fn test_pending() {
        let stream = InputStream::from(SocketInputStream::new_mock());
        stream.set_pending().unwrap();
        assert!(stream.has_pending());
        stream.clear_pending();
        assert!(!stream.has_pending());
    }
}
