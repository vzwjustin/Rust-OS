//! GZlibDecompressor matching `gio/gzlibdecompressor.h`.
//!
//! Upstream `GZlibDecompressor` implements `GConverter` for zlib/gzip
//! decompression. Uses `miniz_oxide` for inflate in zlib, gzip, and raw modes.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gconverter::{Converter, ConverterFlags, ConverterResult};
use crate::gfile::FileInfo;
use crate::gioerror::{io_error_quark, IOErrorEnum};
use crate::gzlibcompressor::ZlibCompressorFormat;
use alloc::vec::Vec;
use miniz_oxide::inflate::{
    decompress_to_vec, decompress_to_vec_zlib, DecompressError, TINFLStatus,
};
use spin::Mutex;

/// Internal decompression state for incremental `Converter` use.
#[derive(Debug, Default)]
struct DecompressState {
    pending_input: Vec<u8>,
    decompressed: Vec<u8>,
    output_pos: usize,
    finished: bool,
}

/// A zlib decompressor (`GZlibDecompressor`).
///
/// Implements `Converter` for decompression using `miniz_oxide`.
pub struct ZlibDecompressor {
    format: ZlibCompressorFormat,
    file_info: Mutex<Option<FileInfo>>,
    state: Mutex<DecompressState>,
}

impl ZlibDecompressor {
    /// Creates a new zlib decompressor.
    ///
    /// Mirrors `g_zlib_decompressor_new`.
    pub fn new(format: ZlibCompressorFormat) -> Self {
        Self {
            format,
            file_info: Mutex::new(None),
            state: Mutex::new(DecompressState::default()),
        }
    }

    /// Gets the file info extracted from the decompressed stream.
    ///
    /// Mirrors `g_zlib_decompressor_get_file_info`.
    pub fn get_file_info(&self) -> Option<FileInfo> {
        self.file_info.lock().clone()
    }

    /// Gets the decompression format.
    pub fn get_format(&self) -> ZlibCompressorFormat {
        self.format
    }

    fn decompress_all(&self, input: &[u8]) -> Result<Vec<u8>, Error> {
        let result = match self.format {
            ZlibCompressorFormat::Zlib => decompress_to_vec_zlib(input),
            ZlibCompressorFormat::Gzip => decompress_gzip(input),
            ZlibCompressorFormat::Raw => decompress_to_vec(input),
        };
        result.map_err(|_| {
            Error::new(
                io_error_quark(),
                IOErrorEnum::InvalidData.to_code(),
                "zlib decompression failed",
            )
        })
    }
}

fn decompress_gzip(input: &[u8]) -> Result<Vec<u8>, DecompressError> {
    if input.len() < 18 || input[0] != 0x1f || input[1] != 0x8b {
        return Err(DecompressError {
            status: TINFLStatus::Failed,
            output: Vec::new(),
        });
    }

    let mut pos = 10usize;
    let flg = input[3];
    if flg & 0x04 != 0 {
        if pos + 2 > input.len() {
            return Err(DecompressError {
                status: TINFLStatus::Failed,
                output: Vec::new(),
            });
        }
        let xlen = u16::from_le_bytes([input[pos], input[pos + 1]]) as usize;
        pos += 2 + xlen;
    }
    if flg & 0x08 != 0 {
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    if flg & 0x10 != 0 {
        while pos < input.len() && input[pos] != 0 {
            pos += 1;
        }
        pos += 1;
    }
    if flg & 0x02 != 0 {
        pos += 2;
    }

    if pos + 8 > input.len() {
        return Err(DecompressError {
            status: TINFLStatus::Failed,
            output: Vec::new(),
        });
    }

    let trailer_start = input.len() - 8;
    decompress_to_vec(&input[pos..trailer_start])
}

impl Converter for ZlibDecompressor {
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

        if state.decompressed.is_empty() {
            if !input_at_end && !flush {
                if inbuf.is_empty() {
                    return Ok((ConverterResult::Error, 0, 0));
                }
                return Ok((ConverterResult::Converted, inbuf.len(), 0));
            }
            state.decompressed = self.decompress_all(&state.pending_input)?;
            state.output_pos = 0;
        }

        let remaining = state.decompressed.len() - state.output_pos;
        let to_copy = remaining.min(outbuf.len());
        outbuf[..to_copy]
            .copy_from_slice(&state.decompressed[state.output_pos..state.output_pos + to_copy]);
        state.output_pos += to_copy;

        if state.output_pos >= state.decompressed.len() {
            state.finished = true;
            Ok((ConverterResult::Finished, inbuf.len(), to_copy))
        } else {
            Ok((ConverterResult::Converted, inbuf.len(), to_copy))
        }
    }

    fn reset(&self) {
        *self.state.lock() = DecompressState::default();
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gzlibcompressor::ZlibCompressor;

    #[test]
    fn test_zlib_decompressor_new() {
        let decomp = ZlibDecompressor::new(ZlibCompressorFormat::Gzip);
        assert_eq!(decomp.get_format(), ZlibCompressorFormat::Gzip);
    }

    #[test]
    fn test_zlib_decompressor_convert() {
        let comp = ZlibCompressor::new(ZlibCompressorFormat::Zlib, 6);
        let input = b"hello world";
        let mut compressed = [0u8; 64];
        let (_, _, written) = comp
            .convert(input, &mut compressed, ConverterFlags::InputAtEnd)
            .unwrap();

        let decomp = ZlibDecompressor::new(ZlibCompressorFormat::Zlib);
        let mut output = [0u8; 64];
        let (result, read, written_out) = decomp
            .convert(
                &compressed[..written],
                &mut output,
                ConverterFlags::InputAtEnd,
            )
            .unwrap();
        assert_eq!(result, ConverterResult::Finished);
        assert_eq!(read, written);
        assert_eq!(&output[..written_out], input);
    }

    #[test]
    fn test_zlib_decompressor_partial() {
        let comp = ZlibCompressor::new(ZlibCompressorFormat::Raw, 6);
        let input = b"hello world";
        let mut compressed = [0u8; 64];
        let (_, _, written) = comp
            .convert(input, &mut compressed, ConverterFlags::InputAtEnd)
            .unwrap();

        let decomp = ZlibDecompressor::new(ZlibCompressorFormat::Raw);
        let mut output = [0u8; 5];
        let (result, _, written_out) = decomp
            .convert(
                &compressed[..written],
                &mut output,
                ConverterFlags::InputAtEnd,
            )
            .unwrap();
        assert_eq!(result, ConverterResult::Converted);
        assert_eq!(written_out, 5);
        assert_eq!(&output, b"hello");

        let mut rest = [0u8; 16];
        let (result2, _, written2) = decomp
            .convert(&[], &mut rest, ConverterFlags::NoFlags)
            .unwrap();
        assert_eq!(result2, ConverterResult::Finished);
        assert_eq!(&rest[..written2], b" world");
    }

    #[test]
    fn test_zlib_decompressor_file_info_none() {
        let decomp = ZlibDecompressor::new(ZlibCompressorFormat::Gzip);
        assert!(decomp.get_file_info().is_none());
    }

    #[test]
    fn test_zlib_decompressor_reset() {
        let decomp = ZlibDecompressor::new(ZlibCompressorFormat::Zlib);
        decomp.reset();
    }
}
