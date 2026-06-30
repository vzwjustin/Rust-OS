//! GZlibCompressor matching `gio/gzlibcompressor.h`.
//!
//! Upstream `GZlibCompressor` implements `GConverter` for zlib/gzip
//! compression. Uses `miniz_oxide` for deflate in zlib, gzip, and raw modes.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gconverter::{Converter, ConverterFlags, ConverterResult};
use crate::gfile::FileInfo;
use alloc::vec::Vec;
use miniz_oxide::deflate::{compress_to_vec, compress_to_vec_zlib};
use spin::Mutex;

/// Compression format (`GZlibCompressorFormat`).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZlibCompressorFormat {
    Zlib = 0,
    Gzip = 1,
    Raw = 2,
}

/// Internal compression state for incremental `Converter` use.
#[derive(Debug, Default)]
struct CompressState {
    pending_input: Vec<u8>,
    compressed: Vec<u8>,
    output_pos: usize,
    finished: bool,
}

/// A zlib compressor (`GZlibCompressor`).
///
/// Implements `Converter` for compression using `miniz_oxide`.
pub struct ZlibCompressor {
    format: ZlibCompressorFormat,
    level: i32,
    file_info: Mutex<Option<FileInfo>>,
    os: Mutex<i32>,
    state: Mutex<CompressState>,
}

impl ZlibCompressor {
    /// Creates a new zlib compressor.
    ///
    /// Mirrors `g_zlib_compressor_new`.
    pub fn new(format: ZlibCompressorFormat, level: i32) -> Self {
        Self {
            format,
            level,
            file_info: Mutex::new(None),
            os: Mutex::new(3), // Default: Unix
            state: Mutex::new(CompressState::default()),
        }
    }

    /// Gets the file info associated with the compressor.
    ///
    /// Mirrors `g_zlib_compressor_get_file_info`.
    pub fn get_file_info(&self) -> Option<FileInfo> {
        self.file_info.lock().clone()
    }

    /// Sets the file info for the compressor.
    ///
    /// Mirrors `g_zlib_compressor_set_file_info`.
    pub fn set_file_info(&self, file_info: Option<FileInfo>) {
        *self.file_info.lock() = file_info;
    }

    /// Gets the OS field.
    ///
    /// Mirrors `g_zlib_compressor_get_os` (since 2.86).
    pub fn get_os(&self) -> i32 {
        *self.os.lock()
    }

    /// Sets the OS field.
    ///
    /// Mirrors `g_zlib_compressor_set_os` (since 2.86).
    pub fn set_os(&self, os: i32) {
        *self.os.lock() = os;
    }

    /// Gets the compression format.
    pub fn get_format(&self) -> ZlibCompressorFormat {
        self.format
    }

    /// Gets the compression level.
    pub fn get_level(&self) -> i32 {
        self.level
    }

    fn normalize_level(level: i32) -> u8 {
        match level {
            ..=0 => 6,
            1..=10 => level as u8,
            _ => 9,
        }
    }

    fn compress_all(&self, input: &[u8]) -> Vec<u8> {
        let level = Self::normalize_level(self.level);
        match self.format {
            ZlibCompressorFormat::Zlib => compress_to_vec_zlib(input, level),
            ZlibCompressorFormat::Gzip => compress_gzip(input, level, self.get_os() as u8),
            ZlibCompressorFormat::Raw => compress_to_vec(input, level),
        }
    }
}

/// IEEE CRC-32 used by the gzip trailer.
fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// Wrap raw deflate data in a minimal gzip container (RFC 1952).
fn compress_gzip(input: &[u8], level: u8, os: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() + 18);
    out.extend_from_slice(&[0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0, os]);
    out.extend_from_slice(&compress_to_vec(input, level));
    out.extend_from_slice(&crc32_ieee(input).to_le_bytes());
    out.extend_from_slice(&(input.len() as u32).to_le_bytes());
    out
}

impl Converter for ZlibCompressor {
    fn convert(
        &self,
        inbuf: &[u8],
        outbuf: &mut [u8],
        flags: ConverterFlags,
    ) -> Result<(ConverterResult, usize, usize), Error> {
        let input_at_end = (flags as u32) & (ConverterFlags::InputAtEnd as u32) != 0;
        let flush = (flags as u32) & (ConverterFlags::Flush as u32) != 0;

        let mut state = self.state.lock();

        if state.finished {
            return Ok((ConverterResult::Finished, 0, 0));
        }

        if !inbuf.is_empty() {
            state.pending_input.extend_from_slice(inbuf);
        }

        if state.compressed.is_empty() {
            if !input_at_end && !flush {
                if inbuf.is_empty() {
                    return Ok((ConverterResult::Error, 0, 0));
                }
                return Ok((ConverterResult::Converted, inbuf.len(), 0));
            }
            state.compressed = self.compress_all(&state.pending_input);
            state.output_pos = 0;
        }

        let remaining = state.compressed.len() - state.output_pos;
        let to_copy = remaining.min(outbuf.len());
        outbuf[..to_copy]
            .copy_from_slice(&state.compressed[state.output_pos..state.output_pos + to_copy]);
        state.output_pos += to_copy;

        if state.output_pos >= state.compressed.len() {
            state.finished = true;
            Ok((ConverterResult::Finished, inbuf.len(), to_copy))
        } else {
            Ok((ConverterResult::Converted, inbuf.len(), to_copy))
        }
    }

    fn reset(&self) {
        *self.state.lock() = CompressState::default();
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gzlibdecompressor::ZlibDecompressor;

    fn convert_all(comp: &ZlibCompressor, input: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        let mut pos = 0;
        loop {
            let chunk = &input[pos..];
            let mut buf = [0u8; 64];
            let flags = if pos + chunk.len() >= input.len() {
                ConverterFlags::InputAtEnd
            } else {
                ConverterFlags::NoFlags
            };
            let (result, read, written) = comp.convert(chunk, &mut buf, flags).unwrap();
            output.extend_from_slice(&buf[..written]);
            pos += read;
            if result == ConverterResult::Finished {
                break;
            }
        }
        output
    }

    fn roundtrip(format: ZlibCompressorFormat, input: &[u8]) {
        let comp = ZlibCompressor::new(format, 6);
        let compressed = convert_all(&comp, input);
        assert!(
            !compressed.is_empty(),
            "compressed output should not be empty"
        );
        if format != ZlibCompressorFormat::Raw {
            assert_ne!(
                compressed, input,
                "compressed output should differ from input"
            );
        }

        let decomp = ZlibDecompressor::new(format);
        let mut decompressed = Vec::new();
        let mut pos = 0;
        loop {
            let chunk = &compressed[pos..];
            let mut buf = [0u8; 64];
            let flags = if pos + chunk.len() >= compressed.len() {
                ConverterFlags::InputAtEnd
            } else {
                ConverterFlags::NoFlags
            };
            let (result, read, written) = decomp.convert(chunk, &mut buf, flags).unwrap();
            decompressed.extend_from_slice(&buf[..written]);
            pos += read;
            if result == ConverterResult::Finished {
                break;
            }
        }
        assert_eq!(decompressed, input);
    }

    #[test]
    fn test_zlib_compressor_new() {
        let comp = ZlibCompressor::new(ZlibCompressorFormat::Gzip, 6);
        assert_eq!(comp.get_format(), ZlibCompressorFormat::Gzip);
        assert_eq!(comp.get_level(), 6);
    }

    #[test]
    fn test_zlib_compressor_format_values() {
        assert_eq!(ZlibCompressorFormat::Zlib as u32, 0);
        assert_eq!(ZlibCompressorFormat::Gzip as u32, 1);
        assert_eq!(ZlibCompressorFormat::Raw as u32, 2);
    }

    #[test]
    fn test_zlib_compressor_convert() {
        let comp = ZlibCompressor::new(ZlibCompressorFormat::Zlib, -1);
        let input = b"hello world";
        let mut output = [0u8; 64];
        let (result, read, written) = comp
            .convert(input, &mut output, ConverterFlags::InputAtEnd)
            .unwrap();
        assert_eq!(result, ConverterResult::Finished);
        assert_eq!(read, 11);
        assert!(written > 0);
        assert_ne!(&output[..written], input);
    }

    #[test]
    fn test_zlib_compressor_partial_output() {
        let comp = ZlibCompressor::new(ZlibCompressorFormat::Zlib, -1);
        let input = b"hello world this is a longer string for compression";
        let mut small = [0u8; 8];
        let (result, read, written) = comp
            .convert(input, &mut small, ConverterFlags::InputAtEnd)
            .unwrap();
        assert_eq!(result, ConverterResult::Converted);
        assert_eq!(read, input.len());
        assert_eq!(written, 8);

        let mut rest = [0u8; 128];
        let (result2, read2, written2) = comp
            .convert(&[], &mut rest, ConverterFlags::NoFlags)
            .unwrap();
        assert_eq!(result2, ConverterResult::Finished);
        assert_eq!(read2, 0);
        assert!(written2 > 0);
    }

    #[test]
    fn test_zlib_compressor_os() {
        let comp = ZlibCompressor::new(ZlibCompressorFormat::Gzip, 6);
        assert_eq!(comp.get_os(), 3); // Default: Unix
        comp.set_os(0); // FAT
        assert_eq!(comp.get_os(), 0);
    }

    #[test]
    fn test_zlib_compressor_file_info_none() {
        let comp = ZlibCompressor::new(ZlibCompressorFormat::Gzip, 6);
        assert!(comp.get_file_info().is_none());
    }

    #[test]
    fn test_zlib_compressor_reset() {
        let comp = ZlibCompressor::new(ZlibCompressorFormat::Zlib, -1);
        let input = b"hello";
        let mut output = [0u8; 64];
        comp.convert(input, &mut output, ConverterFlags::InputAtEnd)
            .unwrap();
        comp.reset();
        let (result, _, _) = comp
            .convert(input, &mut output, ConverterFlags::InputAtEnd)
            .unwrap();
        assert_eq!(result, ConverterResult::Finished);
    }

    #[test]
    fn roundtrip_zlib() {
        roundtrip(ZlibCompressorFormat::Zlib, b"hello world");
    }

    #[test]
    fn roundtrip_gzip() {
        roundtrip(
            ZlibCompressorFormat::Gzip,
            b"the quick brown fox jumps over the lazy dog",
        );
    }

    #[test]
    fn roundtrip_raw() {
        roundtrip(
            ZlibCompressorFormat::Raw,
            b"raw deflate roundtrip test data",
        );
    }
}
