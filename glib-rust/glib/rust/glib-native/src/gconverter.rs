//! GConverter interface matching `gio/gconverter.h`.
//!
//! Upstream `GConverter` is a `GInterface` for converting data from one
//! type to another (e.g. compression/decompression). We port it as a
//! Rust trait with associated enums.
//!
//! Provides:
//! - `ConverterFlags` bitflag enum (NoFlags/InputAtEnd/Flush).
//! - `ConverterResult` enum (Error/Converted/Finished/Flushed).
//! - `Converter` trait (convert/reset).
//!
//! Fully `no_std` compatible.

use crate::error::Error;

/// Flags for converter operations (`GConverterFlags`).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConverterFlags {
    NoFlags = 0,
    InputAtEnd = 1 << 0,
    Flush = 1 << 1,
}

/// Result of a converter operation (`GConverterResult`).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConverterResult {
    Error = 0,
    Converted = 1,
    Finished = 2,
    Flushed = 3,
}

/// Trait for data converters (`GConverter`).
///
/// Implementations convert data from one type to another. The conversion
/// can be stateful and may fail at any point.
pub trait Converter {
    /// Converts data from `inbuf` to `outbuf`.
    ///
    /// Mirrors `g_converter_convert`.
    /// Returns `(ConverterResult, bytes_read, bytes_written)`.
    fn convert(
        &self,
        inbuf: &[u8],
        outbuf: &mut [u8],
        flags: ConverterFlags,
    ) -> Result<(ConverterResult, usize, usize), Error>;

    /// Resets the converter to its initial state.
    ///
    /// Mirrors `g_converter_reset`.
    fn reset(&self);
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
            flags: ConverterFlags,
        ) -> Result<(ConverterResult, usize, usize), Error> {
            let to_copy = inbuf.len().min(outbuf.len());
            outbuf[..to_copy].copy_from_slice(&inbuf[..to_copy]);
            let result = if (flags as u32) & (ConverterFlags::InputAtEnd as u32) != 0
                && to_copy == inbuf.len()
            {
                ConverterResult::Finished
            } else if to_copy > 0 {
                ConverterResult::Converted
            } else {
                ConverterResult::Error
            };
            Ok((result, to_copy, to_copy))
        }

        fn reset(&self) {}
    }

    #[test]
    fn test_converter_flags_values() {
        assert_eq!(ConverterFlags::NoFlags as u32, 0);
        assert_eq!(ConverterFlags::InputAtEnd as u32, 1);
        assert_eq!(ConverterFlags::Flush as u32, 2);
    }

    #[test]
    fn test_converter_result_values() {
        assert_eq!(ConverterResult::Error as u32, 0);
        assert_eq!(ConverterResult::Converted as u32, 1);
        assert_eq!(ConverterResult::Finished as u32, 2);
        assert_eq!(ConverterResult::Flushed as u32, 3);
    }

    #[test]
    fn test_identity_converter_basic() {
        let conv = IdentityConverter;
        let input = b"hello";
        let mut output = [0u8; 5];
        let (result, read, written) = conv
            .convert(input, &mut output, ConverterFlags::NoFlags)
            .unwrap();
        assert_eq!(result, ConverterResult::Converted);
        assert_eq!(read, 5);
        assert_eq!(written, 5);
        assert_eq!(&output, b"hello");
    }

    #[test]
    fn test_identity_converter_finished() {
        let conv = IdentityConverter;
        let input = b"hi";
        let mut output = [0u8; 2];
        let (result, _, _) = conv
            .convert(input, &mut output, ConverterFlags::InputAtEnd)
            .unwrap();
        assert_eq!(result, ConverterResult::Finished);
    }

    #[test]
    fn test_identity_converter_partial() {
        let conv = IdentityConverter;
        let input = b"hello";
        let mut output = [0u8; 3];
        let (result, read, written) = conv
            .convert(input, &mut output, ConverterFlags::NoFlags)
            .unwrap();
        assert_eq!(result, ConverterResult::Converted);
        assert_eq!(read, 3);
        assert_eq!(written, 3);
        assert_eq!(&output, b"hel");
    }
}
