//! GCharsetConverter matching `gio/gcharsetconverter.h`.
//!
//! Upstream `GCharsetConverter` implements `GConverter` for charset
//! conversion (e.g. UTF-8 to ISO-8859-1). We port it as a struct
//! implementing the `Converter` trait with a simple identity passthrough
//! for UTF-8 to UTF-8 and basic fallback support.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gconverter::{Converter, ConverterFlags, ConverterResult};
use alloc::string::{String, ToString};
use spin::Mutex;

/// A charset converter (`GCharsetConverter`).
///
/// Converts data between character encodings. Currently supports
/// identity conversion (same charset) and a basic passthrough mode.
pub struct CharsetConverter {
    to_charset: String,
    from_charset: String,
    use_fallback: Mutex<bool>,
    num_fallbacks: Mutex<u32>,
}

impl CharsetConverter {
    /// Creates a new charset converter.
    ///
    /// Mirrors `g_charset_converter_new`.
    pub fn new(to_charset: &str, from_charset: &str) -> Result<Self, Error> {
        if to_charset.is_empty() || from_charset.is_empty() {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::InvalidArgument.to_code(),
                "Charset names must not be empty",
            ));
        }
        Ok(Self {
            to_charset: to_charset.to_string(),
            from_charset: from_charset.to_string(),
            use_fallback: Mutex::new(false),
            num_fallbacks: Mutex::new(0),
        })
    }

    /// Sets whether to use fallback characters for unconvertible bytes.
    ///
    /// Mirrors `g_charset_converter_set_use_fallback`.
    pub fn set_use_fallback(&self, use_fallback: bool) {
        *self.use_fallback.lock() = use_fallback;
    }

    /// Gets whether fallback is enabled.
    ///
    /// Mirrors `g_charset_converter_get_use_fallback`.
    pub fn get_use_fallback(&self) -> bool {
        *self.use_fallback.lock()
    }

    /// Gets the number of fallback characters used so far.
    ///
    /// Mirrors `g_charset_converter_get_num_fallbacks`.
    pub fn get_num_fallbacks(&self) -> u32 {
        *self.num_fallbacks.lock()
    }

    /// Gets the target charset.
    pub fn get_to_charset(&self) -> &str {
        &self.to_charset
    }

    /// Gets the source charset.
    pub fn get_from_charset(&self) -> &str {
        &self.from_charset
    }
}

impl Converter for CharsetConverter {
    fn convert(
        &self,
        inbuf: &[u8],
        outbuf: &mut [u8],
        flags: ConverterFlags,
    ) -> Result<(ConverterResult, usize, usize), Error> {
        // For identity conversion (same charset), just copy bytes
        if self.to_charset.eq_ignore_ascii_case(&self.from_charset) {
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

            return Ok((result, to_copy, to_copy));
        }

        // For UTF-8 to ASCII: replace non-ASCII with '?' if fallback is enabled
        if self.to_charset.eq_ignore_ascii_case("ASCII")
            && self.from_charset.eq_ignore_ascii_case("UTF-8")
        {
            let mut written = 0;
            let mut read = 0;
            let fallback = *self.use_fallback.lock();

            while read < inbuf.len() && written < outbuf.len() {
                let b = inbuf[read];
                if b < 0x80 {
                    outbuf[written] = b;
                    read += 1;
                    written += 1;
                } else if fallback {
                    outbuf[written] = b'?';
                    read += 1;
                    written += 1;
                    *self.num_fallbacks.lock() += 1;
                } else {
                    return Err(Error::new(
                        crate::gioerror::io_error_quark(),
                        crate::gioerror::IOErrorEnum::InvalidData.to_code(),
                        "Non-ASCII byte encountered without fallback",
                    ));
                }
            }

            let result = if (flags as u32) & (ConverterFlags::InputAtEnd as u32) != 0
                && read == inbuf.len()
            {
                ConverterResult::Finished
            } else if written > 0 {
                ConverterResult::Converted
            } else {
                ConverterResult::Error
            };

            return Ok((result, read, written));
        }

        // For unsupported conversions, return an error
        Err(Error::new(
            crate::gioerror::io_error_quark(),
            crate::gioerror::IOErrorEnum::NotSupported.to_code(),
            "Charset conversion not supported for the given pair",
        ))
    }

    fn reset(&self) {
        *self.num_fallbacks.lock() = 0;
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_charset_converter_new() {
        let conv = CharsetConverter::new("UTF-8", "UTF-8").unwrap();
        assert_eq!(conv.get_to_charset(), "UTF-8");
        assert_eq!(conv.get_from_charset(), "UTF-8");
        assert!(!conv.get_use_fallback());
        assert_eq!(conv.get_num_fallbacks(), 0);
    }

    #[test]
    fn test_charset_converter_empty_error() {
        assert!(CharsetConverter::new("", "UTF-8").is_err());
        assert!(CharsetConverter::new("UTF-8", "").is_err());
    }

    #[test]
    fn test_charset_converter_identity() {
        let conv = CharsetConverter::new("UTF-8", "UTF-8").unwrap();
        let input = b"hello world";
        let mut output = [0u8; 11];
        let (result, read, written) = conv
            .convert(input, &mut output, ConverterFlags::InputAtEnd)
            .unwrap();
        assert_eq!(result, ConverterResult::Finished);
        assert_eq!(read, 11);
        assert_eq!(written, 11);
        assert_eq!(&output, b"hello world");
    }

    #[test]
    fn test_charset_converter_set_use_fallback() {
        let conv = CharsetConverter::new("UTF-8", "UTF-8").unwrap();
        conv.set_use_fallback(true);
        assert!(conv.get_use_fallback());
    }

    #[test]
    fn test_charset_converter_utf8_to_ascii() {
        let conv = CharsetConverter::new("ASCII", "UTF-8").unwrap();
        conv.set_use_fallback(true);
        let input = b"hello\xc3\xa9"; // "helloé" in UTF-8 (7 bytes)
        let mut output = [0u8; 7];
        let (result, read, written) = conv
            .convert(input, &mut output, ConverterFlags::InputAtEnd)
            .unwrap();
        assert_eq!(result, ConverterResult::Finished);
        assert_eq!(read, 7);
        assert_eq!(written, 7);
        assert_eq!(&output, b"hello??");
        assert_eq!(conv.get_num_fallbacks(), 2);
    }

    #[test]
    fn test_charset_converter_utf8_to_ascii_no_fallback() {
        let conv = CharsetConverter::new("ASCII", "UTF-8").unwrap();
        let input = b"hello\xc3\xa9";
        let mut output = [0u8; 7];
        assert!(conv
            .convert(input, &mut output, ConverterFlags::InputAtEnd)
            .is_err());
    }

    #[test]
    fn test_charset_converter_reset() {
        let conv = CharsetConverter::new("ASCII", "UTF-8").unwrap();
        conv.set_use_fallback(true);
        let input = b"\xc3\xa9";
        let mut output = [0u8; 2];
        let _ = conv
            .convert(input, &mut output, ConverterFlags::InputAtEnd)
            .unwrap();
        assert_eq!(conv.get_num_fallbacks(), 2);
        conv.reset();
        assert_eq!(conv.get_num_fallbacks(), 0);
    }

    #[test]
    fn test_charset_converter_unsupported() {
        let conv = CharsetConverter::new("ISO-8859-1", "SHIFT-JIS").unwrap();
        let input = b"test";
        let mut output = [0u8; 4];
        assert!(conv
            .convert(input, &mut output, ConverterFlags::NoFlags)
            .is_err());
    }
}
