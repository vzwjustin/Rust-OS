//! GIO I/O stream matching `gio/giostream.h` / `gio/giostream.c`.
//!
//! Provides the `IOStream` wrapper that combines an `InputStream` and an
//! `OutputStream` for bidirectional communication.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginputstream::InputStream;
use crate::gioerror::IOErrorEnum;
use crate::goutputstream::OutputStream;
use spin::Mutex;

struct IOStreamState {
    closed: bool,
    pending: bool,
}

/// A bidirectional I/O stream (`GIOStream`).
pub struct IOStream {
    input_stream: InputStream,
    output_stream: OutputStream,
    state: Mutex<IOStreamState>,
}

impl IOStream {
    /// Create a new `IOStream` combining `input_stream` and `output_stream`.
    ///
    /// Mirrors `g_io_stream_new`.
    pub fn new(input_stream: InputStream, output_stream: OutputStream) -> Self {
        Self {
            input_stream,
            output_stream,
            state: Mutex::new(IOStreamState {
                closed: false,
                pending: false,
            }),
        }
    }

    /// Gets the input stream portion of the `IOStream`.
    ///
    /// Mirrors `g_io_stream_get_input_stream`.
    pub fn get_input_stream(&self) -> InputStream {
        self.input_stream.clone()
    }

    /// Gets the output stream portion of the `IOStream`.
    ///
    /// Mirrors `g_io_stream_get_output_stream`.
    pub fn get_output_stream(&self) -> OutputStream {
        self.output_stream.clone()
    }

    /// Close the I/O stream and its child streams.
    ///
    /// Mirrors `g_io_stream_close`.
    pub fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let mut state = self.state.lock();
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        if state.closed {
            return Ok(());
        }
        state.closed = true;

        self.input_stream.close(cancellable)?;
        self.output_stream.close(cancellable)?;
        Ok(())
    }

    /// Checks if the stream is closed.
    ///
    /// Mirrors `g_io_stream_is_closed`.
    pub fn is_closed(&self) -> bool {
        self.state.lock().closed
    }

    /// Checks if the stream has a pending operation.
    ///
    /// Mirrors `g_io_stream_has_pending`.
    pub fn has_pending(&self) -> bool {
        self.state.lock().pending
    }

    /// Sets the pending operation state.
    ///
    /// Mirrors `g_io_stream_set_pending`.
    pub fn set_pending(&self) -> Result<(), Error> {
        let mut state = self.state.lock();
        if state.closed {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::Closed.to_code(),
                "IOStream is closed",
            ));
        }
        if state.pending {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::Pending.to_code(),
                "IOStream has pending operation",
            ));
        }
        state.pending = true;
        Ok(())
    }

    /// Clears the pending operation state.
    ///
    /// Mirrors `g_io_stream_clear_pending`.
    pub fn clear_pending(&self) {
        self.state.lock().pending = false;
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ginputstream::MemoryInputStream;
    use crate::goutputstream::MemoryOutputStream;

    #[test]
    fn test_iostream_creation_and_streams() {
        let in_stream = InputStream::from(MemoryInputStream::new());
        let out_stream = OutputStream::from(MemoryOutputStream::new_resizable());
        let io_stream = IOStream::new(in_stream.clone(), out_stream.clone());

        assert!(!io_stream.is_closed());
        let retrieved_in = io_stream.get_input_stream();
        let retrieved_out = io_stream.get_output_stream();

        // Verify they wrap the same underlying streams (is_closed shares state)
        assert!(!retrieved_in.is_closed());
        assert!(!retrieved_out.is_closed());
    }

    #[test]
    fn test_iostream_close() {
        let in_stream = InputStream::from(MemoryInputStream::new());
        let out_stream = OutputStream::from(MemoryOutputStream::new_resizable());
        let io_stream = IOStream::new(in_stream.clone(), out_stream.clone());

        io_stream.close(None).unwrap();
        assert!(io_stream.is_closed());
        assert!(in_stream.is_closed());
        assert!(out_stream.is_closed());
    }

    #[test]
    fn test_iostream_pending() {
        let in_stream = InputStream::from(MemoryInputStream::new());
        let out_stream = OutputStream::from(MemoryOutputStream::new_resizable());
        let io_stream = IOStream::new(in_stream, out_stream);

        assert!(!io_stream.has_pending());
        io_stream.set_pending().unwrap();
        assert!(io_stream.has_pending());
        assert_eq!(
            io_stream.set_pending().unwrap_err().code(),
            IOErrorEnum::Pending.to_code()
        );
        io_stream.clear_pending();
        assert!(!io_stream.has_pending());
    }

    #[test]
    fn test_iostream_close_cancellation() {
        let in_stream = InputStream::from(MemoryInputStream::new());
        let out_stream = OutputStream::from(MemoryOutputStream::new_resizable());
        let io_stream = IOStream::new(in_stream, out_stream);
        let cancellable = GCancellable::new();
        cancellable.cancel();

        assert_eq!(
            io_stream.close(Some(&cancellable)).unwrap_err().code(),
            IOErrorEnum::Cancelled.to_code()
        );
    }
}
