//! GSocketOutputStream matching `gio/gsocketoutputstream.h` / `gio/gsocketoutputstream.c`.
//!
//! A `GOutputStream` backed by a `GSocket`. On bare-metal `no_std` targets the
//! underlying socket is modelled via the [`Socket`] trait (typically
//! [`MockSocket`]); transmitted bytes are forwarded through `send()`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::gioerror::{io_error_quark, IOErrorEnum};
use crate::goutputstream::{OutputStream, OutputStreamImpl};
use crate::gsocket::{MockSocket, Socket};
use alloc::sync::Arc;
use spin::Mutex;

/// Internal state for [`SocketOutputStream`].
struct SocketOutputStreamState {
    stream_closed: bool,
    pending: bool,
}

/// A socket-backed output stream (`GSocketOutputStream`).
pub struct SocketOutputStream {
    socket: Arc<dyn Socket + Send + Sync>,
    state: Mutex<SocketOutputStreamState>,
}

impl SocketOutputStream {
    /// Creates a new `SocketOutputStream` wrapping `socket`.
    ///
    /// Mirrors `g_socket_output_stream_new`.
    pub fn new<S: Socket + Send + Sync + 'static>(socket: S) -> Self {
        Self::new_arc(Arc::new(socket))
    }

    /// Creates a new `SocketOutputStream` from an existing `Arc` socket.
    pub fn new_arc(socket: Arc<dyn Socket + Send + Sync>) -> Self {
        Self {
            socket,
            state: Mutex::new(SocketOutputStreamState {
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
    /// Mirrors `g_socket_output_stream_get_socket`.
    pub fn get_socket(&self) -> Arc<dyn Socket + Send + Sync> {
        Arc::clone(&self.socket)
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
        let mut total_written = 0;
        let mut err = None;
        while total_written < buffer.len() {
            match self.write(&buffer[total_written..], cancellable) {
                Ok(0) => break,
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

    /// Mirrors `g_output_stream_flush`.
    pub fn flush(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        OutputStreamImpl::flush(self, cancellable)
    }

    /// Close the stream (does not close the underlying socket).
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

impl Clone for SocketOutputStream {
    fn clone(&self) -> Self {
        Self {
            socket: Arc::clone(&self.socket),
            state: Mutex::new(SocketOutputStreamState {
                stream_closed: self.state.lock().stream_closed,
                pending: self.state.lock().pending,
            }),
        }
    }
}

impl OutputStreamImpl for SocketOutputStream {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn write(&self, buffer: &[u8], cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        let st = self.state.lock();
        if st.stream_closed {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        drop(st);

        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.socket.is_closed() {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "Underlying socket is closed",
            ));
        }
        self.socket.send(buffer, cancellable)
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

impl From<SocketOutputStream> for OutputStream {
    fn from(s: SocketOutputStream) -> Self {
        OutputStream::new(s)
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gsocket::MockSocket;

    #[test]
    fn test_write_via_socket() {
        let mock = MockSocket::new_stream();
        let stream = SocketOutputStream::new(mock);
        assert_eq!(stream.write(b"hello", None).unwrap(), 5);
    }

    #[test]
    fn test_write_all() {
        let stream = SocketOutputStream::new_mock();
        let (n, err) = stream.write_all(b"write all", None).unwrap();
        assert_eq!(n, 9);
        assert!(err.is_none());
    }

    #[test]
    fn test_get_socket() {
        let stream = SocketOutputStream::new_mock();
        assert!(!stream.get_socket().is_closed());
    }

    #[test]
    fn test_flush_and_close() {
        let stream = SocketOutputStream::new_mock();
        stream.flush(None).unwrap();
        assert!(!stream.is_closed());
        stream.close(None).unwrap();
        assert!(stream.is_closed());
        assert_eq!(
            stream.write(b"x", None).unwrap_err().code(),
            IOErrorEnum::Closed.to_code()
        );
    }

    #[test]
    fn test_output_stream_wrapper() {
        let stream = OutputStream::from(SocketOutputStream::new_mock());
        assert_eq!(stream.write(b"data", None).unwrap(), 4);
    }

    #[test]
    fn test_closed_socket_write() {
        let mock = MockSocket::new_stream();
        mock.close(None).unwrap();
        let stream = SocketOutputStream::new(mock);
        assert_eq!(
            stream.write(b"x", None).unwrap_err().code(),
            IOErrorEnum::Closed.to_code()
        );
    }
}
