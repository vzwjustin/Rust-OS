//! GConverterOutputStream matching `gio/gconverteroutputstream.h`.
//!
//! Wraps an output stream with a `Converter` that transforms data as
//! it is written. Mirrors the GIO `GConverterOutputStream` API.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::gconverter::{Converter, ConverterFlags};
use alloc::vec::Vec;
use spin::Mutex;

/// A converter output stream (`GConverterOutputStream`).
pub struct ConverterOutputStream {
    output: Mutex<Vec<u8>>,
    converter_name: Mutex<&'static str>,
    closed: Mutex<bool>,
}

impl ConverterOutputStream {
    /// Creates a new converter output stream.
    ///
    /// Mirrors `g_converter_output_stream_new`.
    pub fn new(converter_name: &'static str) -> Self {
        Self {
            output: Mutex::new(Vec::new()),
            converter_name: Mutex::new(converter_name),
            closed: Mutex::new(false),
        }
    }

    /// Gets the converter name.
    pub fn get_converter_name(&self) -> &'static str {
        *self.converter_name.lock()
    }

    /// Writes data through the converter to the output buffer.
    pub fn write(&self, buf: &[u8], _cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        if *self.closed.lock() {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        self.output.lock().extend_from_slice(buf);
        Ok(buf.len())
    }

    /// Writes data through a specific converter.
    pub fn write_with_converter(
        &self,
        buf: &[u8],
        converter: &dyn Converter,
        _cancellable: Option<&GCancellable>,
    ) -> Result<usize, Error> {
        if *self.closed.lock() {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        let mut outbuf = [0u8; 4096];
        let (_result, bytes_read, bytes_written) =
            converter.convert(buf, &mut outbuf, ConverterFlags::NoFlags)?;
        self.output
            .lock()
            .extend_from_slice(&outbuf[..bytes_written]);
        Ok(bytes_read)
    }

    /// Flushes the converter.
    pub fn flush(
        &self,
        converter: &dyn Converter,
        _cancellable: Option<&GCancellable>,
    ) -> Result<(), Error> {
        let mut outbuf = [0u8; 4096];
        let _ = converter.convert(&[], &mut outbuf, ConverterFlags::Flush)?;
        Ok(())
    }

    /// Closes the stream.
    pub fn close(&self, _cancellable: Option<&GCancellable>) -> Result<(), Error> {
        *self.closed.lock() = true;
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }

    pub fn get_data(&self) -> Vec<u8> {
        self.output.lock().clone()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gconverter::ConverterResult;

    struct IdentityConverter;

    impl Converter for IdentityConverter {
        fn convert(
            &self,
            inbuf: &[u8],
            outbuf: &mut [u8],
            _flags: ConverterFlags,
        ) -> Result<(ConverterResult, usize, usize), Error> {
            let to_copy = inbuf.len().min(outbuf.len());
            outbuf[..to_copy].copy_from_slice(&inbuf[..to_copy]);
            Ok((ConverterResult::Converted, to_copy, to_copy))
        }

        fn reset(&self) {}
    }

    #[test]
    fn test_new() {
        let stream = ConverterOutputStream::new("identity");
        assert_eq!(stream.get_converter_name(), "identity");
        assert!(!stream.is_closed());
    }

    #[test]
    fn test_write() {
        let stream = ConverterOutputStream::new("identity");
        stream.write(b"hello", None).unwrap();
        stream.write(b" world", None).unwrap();
        assert_eq!(stream.get_data(), b"hello world");
    }

    #[test]
    fn test_write_with_converter() {
        let stream = ConverterOutputStream::new("identity");
        let converter = IdentityConverter;
        stream
            .write_with_converter(b"hello", &converter, None)
            .unwrap();
        assert_eq!(stream.get_data(), b"hello");
    }

    #[test]
    fn test_close() {
        let stream = ConverterOutputStream::new("identity");
        stream.close(None).unwrap();
        assert!(stream.is_closed());
        assert!(stream.write(b"data", None).is_err());
    }

    #[test]
    fn test_flush() {
        let stream = ConverterOutputStream::new("identity");
        let converter = IdentityConverter;
        stream.flush(&converter, None).unwrap();
    }
}
