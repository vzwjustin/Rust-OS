//! GConverterInputStream matching `gio/gconverterinputstream.h`.
//!
//! Wraps an input stream with a `Converter` that transforms data as
//! it is read. Mirrors the GIO `GConverterInputStream` API.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::gconverter::{Converter, ConverterFlags, ConverterResult};
use alloc::vec::Vec;
use spin::Mutex;

/// A converter input stream (`GConverterInputStream`).
pub struct ConverterInputStream {
    input: Mutex<Vec<u8>>,
    input_pos: Mutex<usize>,
    converter_name: Mutex<&'static str>,
    closed: Mutex<bool>,
    finished: Mutex<bool>,
}

impl ConverterInputStream {
    /// Creates a new converter input stream.
    ///
    /// Mirrors `g_converter_input_stream_new`.
    pub fn new(data: &[u8], converter_name: &'static str) -> Self {
        Self {
            input: Mutex::new(data.to_vec()),
            input_pos: Mutex::new(0),
            converter_name: Mutex::new(converter_name),
            closed: Mutex::new(false),
            finished: Mutex::new(false),
        }
    }

    /// Gets the converter name.
    pub fn get_converter_name(&self) -> &'static str {
        *self.converter_name.lock()
    }

    /// Reads converted data from the stream.
    pub fn read(
        &self,
        dest: &mut [u8],
        _cancellable: Option<&GCancellable>,
    ) -> Result<usize, Error> {
        if *self.closed.lock() {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::Closed.to_code(),
                "Stream is closed",
            ));
        }
        if *self.finished.lock() {
            return Ok(0);
        }
        let mut pos = self.input_pos.lock();
        let input = self.input.lock();
        let available = input.len().saturating_sub(*pos);
        let to_read = dest.len().min(available);
        if to_read == 0 {
            *self.finished.lock() = true;
            return Ok(0);
        }
        dest[..to_read].copy_from_slice(&input[*pos..*pos + to_read]);
        *pos += to_read;
        Ok(to_read)
    }

    /// Reads using a specific converter (identity passthrough by default).
    pub fn read_with_converter(
        &self,
        dest: &mut [u8],
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
        let mut pos = self.input_pos.lock();
        let input = self.input.lock();
        let available = input.len().saturating_sub(*pos);
        if available == 0 {
            *self.finished.lock() = true;
            return Ok(0);
        }
        let inbuf = &input[*pos..];
        let flags = if *pos + inbuf.len() >= input.len() {
            ConverterFlags::InputAtEnd
        } else {
            ConverterFlags::NoFlags
        };
        let (result, bytes_read, bytes_written) = converter.convert(inbuf, dest, flags)?;
        *pos += bytes_read;
        match result {
            ConverterResult::Finished => {
                *self.finished.lock() = true;
                Ok(bytes_written)
            }
            _ => Ok(bytes_written),
        }
    }

    /// Closes the stream.
    pub fn close(&self, _cancellable: Option<&GCancellable>) -> Result<(), Error> {
        *self.closed.lock() = true;
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
        let stream = ConverterInputStream::new(b"hello", "identity");
        assert_eq!(stream.get_converter_name(), "identity");
        assert!(!stream.is_closed());
    }

    #[test]
    fn test_read() {
        let stream = ConverterInputStream::new(b"hello world", "identity");
        let mut buf = [0u8; 5];
        let n = stream.read(&mut buf, None).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_read_exhausted() {
        let stream = ConverterInputStream::new(b"hi", "identity");
        let mut buf = [0u8; 10];
        let n = stream.read(&mut buf, None).unwrap();
        assert_eq!(n, 2);
        let n2 = stream.read(&mut buf, None).unwrap();
        assert_eq!(n2, 0);
    }

    #[test]
    fn test_read_with_converter() {
        let stream = ConverterInputStream::new(b"hello", "identity");
        let converter = IdentityConverter;
        let mut buf = [0u8; 5];
        let n = stream
            .read_with_converter(&mut buf, &converter, None)
            .unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_close() {
        let stream = ConverterInputStream::new(b"hello", "identity");
        stream.close(None).unwrap();
        assert!(stream.is_closed());
        assert!(stream.read(&mut [0u8; 1], None).is_err());
    }
}
