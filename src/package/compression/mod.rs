//! Compression format support for package extraction
//!
//! This module provides decompression utilities for common package formats.

pub mod gzip;
pub mod tar;

pub use gzip::GzipDecoder;
pub use tar::TarArchive;

use crate::package::{PackageError, PackageResult};
use alloc::vec::Vec;

/// Compression format types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionFormat {
    /// No compression
    None,
    /// Gzip compression
    Gzip,
    /// XZ/LZMA compression
    Xz,
    /// Zstandard compression
    Zstd,
    /// Bzip2 compression
    Bzip2,
}

impl CompressionFormat {
    /// Detect compression format from magic bytes
    pub fn detect(data: &[u8]) -> Self {
        if data.len() < 4 {
            return CompressionFormat::None;
        }

        // Gzip: 1f 8b
        if data[0] == 0x1f && data[1] == 0x8b {
            return CompressionFormat::Gzip;
        }

        // XZ: fd 37 7a 58 5a 00
        if data.len() >= 6
            && data[0] == 0xfd
            && data[1] == 0x37
            && data[2] == 0x7a
            && data[3] == 0x58
            && data[4] == 0x5a
            && data[5] == 0x00
        {
            return CompressionFormat::Xz;
        }

        // Zstd: 28 b5 2f fd
        if data[0] == 0x28 && data[1] == 0xb5 && data[2] == 0x2f && data[3] == 0xfd {
            return CompressionFormat::Zstd;
        }

        // Bzip2: 42 5a 68
        if data[0] == 0x42 && data[1] == 0x5a && data[2] == 0x68 {
            return CompressionFormat::Bzip2;
        }

        CompressionFormat::None
    }
}

/// Decompress data based on detected format
///
/// Gzip/DEFLATE is fully implemented via `miniz_oxide`.
/// XZ, Zstd, and Bzip2 require pure-Rust no_std decoder crates that do
/// not yet exist in the ecosystem — porting liblzma/libzstd/libbz2 C
/// code into the kernel build is the path forward.  Until then these
/// formats return an explicit error stating the missing dependency.
pub fn decompress(data: &[u8]) -> PackageResult<Vec<u8>> {
    let format = CompressionFormat::detect(data);

    match format {
        CompressionFormat::Gzip => GzipDecoder::decode(data),
        CompressionFormat::Xz => Err(PackageError::InvalidFormat(
            "XZ/LZMA2 decompression requires a liblzma port — not available in no_std kernel"
                .into(),
        )),
        CompressionFormat::Zstd => Err(PackageError::InvalidFormat(
            "Zstd decompression requires a libzstd port — not available in no_std kernel".into(),
        )),
        CompressionFormat::Bzip2 => Err(PackageError::InvalidFormat(
            "Bzip2 decompression requires a libbz2 port — not available in no_std kernel".into(),
        )),
        CompressionFormat::None => Ok(data.to_vec()),
    }
}
