//! GSimpleIOStream matching `gio/gsimpleiostream.h`.
//!
//! Bundles a separate input stream and output stream into a single `GIOStream`.
//! No_std compatible; streams are modelled as byte-buffer mocks.

use crate::error::Error;
use crate::ginputstream::InputStream;
use crate::goutputstream::OutputStream;

/// A `GIOStream` that owns a separate `InputStream` and `OutputStream`.
pub struct SimpleIOStream {
    input: InputStream,
    output: OutputStream,
}

impl SimpleIOStream {
    /// Creates a new `SimpleIOStream` from an existing input and output stream.
    ///
    /// Mirrors `g_simple_io_stream_new`.
    pub fn new(input: InputStream, output: OutputStream) -> Self {
        Self { input, output }
    }

    /// Returns a reference to the underlying input stream.
    ///
    /// Mirrors `g_io_stream_get_input_stream`.
    pub fn get_input_stream(&self) -> &InputStream {
        &self.input
    }

    /// Returns a reference to the underlying output stream.
    ///
    /// Mirrors `g_io_stream_get_output_stream`.
    pub fn get_output_stream(&self) -> &OutputStream {
        &self.output
    }

    /// Closes both streams.
    ///
    /// Mirrors `g_io_stream_close`.
    pub fn close(&self) -> Result<(), Error> {
        self.input.close(None)?;
        self.output.close(None)?;
        Ok(())
    }

    /// Returns true if both streams are closed.
    pub fn is_closed(&self) -> bool {
        self.input.is_closed() && self.output.is_closed()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stream() -> SimpleIOStream {
        let bytes = Bytes::from_static(b"hello world");
        let input = InputStream::from(MemoryInputStream::new_from_bytes(bytes));
        let output = OutputStream::from(MemoryOutputStream::new_resizable());
        SimpleIOStream::new(input, output)
    }

    #[test]
    fn test_new() {
        let s = make_stream();
        assert!(!s.is_closed());
    }

    #[test]
    fn test_read_from_input() {
        let s = make_stream();
        let mut buf = [0u8; 5];
        let n = s.get_input_stream().read(&mut buf, None).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_write_to_output() {
        let s = make_stream();
        let n = s.get_output_stream().write(b"data", None).unwrap();
        assert_eq!(n, 4);
    }

    #[test]
    fn test_close() {
        let s = make_stream();
        assert!(!s.is_closed());
        s.close().unwrap();
        assert!(s.is_closed());
    }

    #[test]
    fn test_get_streams() {
        let s = make_stream();
        let _ = s.get_input_stream();
        let _ = s.get_output_stream();
    }
}
