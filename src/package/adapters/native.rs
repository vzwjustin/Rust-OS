//! Native RustOS package adapter
//!
//! This adapter handles the native RustOS package format (`.rustos`), a simple
//! self-describing container optimized for the kernel package manager.
//!
//! Format layout (all multi-byte integers are little-endian):
//!
//! ```text
//! +------------------+  offset 0
//! | magic "RUSTOS\0\0"  (8 bytes)
//! +------------------+  offset 8
//! | format version    u16
//! +------------------+  offset 10
//! | metadata length   u32
//! +------------------+  offset 14
//! | file count        u32
//! +------------------+  offset 18
//! | metadata blob     (metadata length bytes, UTF-8 key=value lines)
//! +------------------+
//! | file entries      (repeated file_count times)
//! |   path length     u16
//! |   path bytes      (path length bytes, UTF-8)
//! |   data length     u32
//! |   data bytes      (data length bytes)
//! +------------------+
//! ```
//!
//! The metadata blob uses `key=value` lines with the same keys understood by
//! `PackageMetadata` (`name`, `version`, `arch`, `description`, `maintainer`,
//! `homepage`, `size`, `installed_size`, and repeated `depend` entries).

use crate::package::adapters::PackageAdapter;
use crate::package::{ExtractedPackage, PackageError, PackageMetadata, PackageResult};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

const NATIVE_MAGIC: &[u8; 8] = b"RUSTOS\0\0";
const NATIVE_FORMAT_VERSION: u16 = 1;
const HEADER_SIZE: usize = 18;

/// Native RustOS package adapter
pub struct NativeAdapter;

impl NativeAdapter {
    /// Create a new native package adapter
    pub fn new() -> Self {
        NativeAdapter
    }

    /// Parse the metadata blob (UTF-8 `key=value` lines) into metadata.
    fn parse_metadata_blob(content: &str) -> PackageResult<PackageMetadata> {
        let mut name = String::new();
        let mut version = String::new();
        let mut architecture = String::from("x86_64");
        let mut description = String::new();
        let mut maintainer = None;
        let mut homepage = None;
        let mut dependencies = Vec::new();
        let mut size = 0u64;
        let mut installed_size = 0u64;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(eq) = line.find('=') {
                let key = line[..eq].trim();
                let value = line[eq + 1..].trim();

                match key {
                    "name" | "pkgname" => name = value.to_string(),
                    "version" | "pkgver" => version = value.to_string(),
                    "arch" | "architecture" => architecture = value.to_string(),
                    "description" | "pkgdesc" => description = value.to_string(),
                    "maintainer" => maintainer = Some(value.to_string()),
                    "homepage" | "url" => homepage = Some(value.to_string()),
                    "depend" => dependencies.push(value.to_string()),
                    "size" => size = value.parse().unwrap_or(0),
                    "installed_size" => installed_size = value.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }

        if name.is_empty() || version.is_empty() {
            return Err(PackageError::InvalidFormat(
                "Native package missing name/version in metadata".to_string(),
            ));
        }

        let mut metadata = PackageMetadata::new(name, version, architecture);
        metadata.description = description;
        metadata.maintainer = maintainer;
        metadata.homepage = homepage;
        metadata.dependencies = dependencies;
        metadata.size = size;
        metadata.installed_size = installed_size;

        Ok(metadata)
    }

    /// Read a little-endian u16 from `data` at `offset`.
    fn read_u16(data: &[u8], offset: usize) -> PackageResult<u16> {
        if offset + 2 > data.len() {
            return Err(PackageError::InvalidFormat(
                "Native package truncated reading u16".to_string(),
            ));
        }
        Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
    }

    /// Read a little-endian u32 from `data` at `offset`.
    fn read_u32(data: &[u8], offset: usize) -> PackageResult<u32> {
        if offset + 4 > data.len() {
            return Err(PackageError::InvalidFormat(
                "Native package truncated reading u32".to_string(),
            ));
        }
        Ok(u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]))
    }

    /// Parse the native header, returning (metadata_len, file_count).
    fn parse_header(data: &[u8]) -> PackageResult<(u32, u32)> {
        if data.len() < HEADER_SIZE {
            return Err(PackageError::InvalidFormat(
                "Native package too small for header".to_string(),
            ));
        }

        if &data[0..8] != NATIVE_MAGIC {
            return Err(PackageError::InvalidFormat(
                "Invalid native package magic".to_string(),
            ));
        }

        let format_version = Self::read_u16(data, 8)?;
        if format_version != NATIVE_FORMAT_VERSION {
            return Err(PackageError::InvalidFormat(format!(
                "Unsupported native package format version: {}",
                format_version
            )));
        }

        let metadata_len = Self::read_u32(data, 10)?;
        let file_count = Self::read_u32(data, 14)?;
        Ok((metadata_len, file_count))
    }

    /// Parse the metadata section starting at `metadata_offset`.
    fn parse_metadata_section(
        data: &[u8],
        metadata_offset: usize,
        metadata_len: u32,
    ) -> PackageResult<PackageMetadata> {
        let end = metadata_offset
            .checked_add(metadata_len as usize)
            .ok_or_else(|| {
                PackageError::InvalidFormat("Native package metadata length overflow".to_string())
            })?;
        if end > data.len() {
            return Err(PackageError::InvalidFormat(
                "Native package metadata section truncated".to_string(),
            ));
        }

        let blob = core::str::from_utf8(&data[metadata_offset..end]).map_err(|_| {
            PackageError::InvalidFormat("Invalid UTF-8 in native package metadata".to_string())
        })?;

        Self::parse_metadata_blob(blob)
    }
}

impl PackageAdapter for NativeAdapter {
    fn extract(&self, data: &[u8]) -> PackageResult<ExtractedPackage> {
        if !self.validate(data)? {
            return Err(PackageError::InvalidFormat(
                "Native RustOS package validation failed".to_string(),
            ));
        }

        let (metadata_len, file_count) = Self::parse_header(data)?;
        let metadata_offset = HEADER_SIZE;
        let metadata = Self::parse_metadata_section(data, metadata_offset, metadata_len)?;

        let mut package = ExtractedPackage::new(metadata);

        // File entries begin right after the metadata blob.
        let mut cursor = metadata_offset + metadata_len as usize;
        for _ in 0..file_count {
            let path_len = Self::read_u16(data, cursor)? as usize;
            cursor += 2;
            if cursor + path_len > data.len() {
                return Err(PackageError::InvalidFormat(
                    "Native package file path truncated".to_string(),
                ));
            }
            let path = core::str::from_utf8(&data[cursor..cursor + path_len])
                .map_err(|_| {
                    PackageError::InvalidFormat(
                        "Invalid UTF-8 in native package file path".to_string(),
                    )
                })?
                .to_string();
            cursor += path_len;

            let data_len = Self::read_u32(data, cursor)? as usize;
            cursor += 4;
            if cursor + data_len > data.len() {
                return Err(PackageError::InvalidFormat(
                    "Native package file data truncated".to_string(),
                ));
            }
            let file_data = data[cursor..cursor + data_len].to_vec();
            cursor += data_len;

            package.add_file(path, file_data);
        }

        Ok(package)
    }

    fn parse_metadata(&self, data: &[u8]) -> PackageResult<PackageMetadata> {
        if !self.validate(data)? {
            return Err(PackageError::InvalidFormat(
                "Native RustOS package validation failed".to_string(),
            ));
        }

        let (metadata_len, _file_count) = Self::parse_header(data)?;
        Self::parse_metadata_section(data, HEADER_SIZE, metadata_len)
    }

    fn validate(&self, data: &[u8]) -> PackageResult<bool> {
        // Native RustOS packages use custom magic number "RUSTOS\0\0".
        if data.len() < 8 {
            return Ok(false);
        }

        Ok(&data[0..6] == b"RUSTOS" && data[6] == 0 && data[7] == 0)
    }

    fn format_name(&self) -> &str {
        "Native RustOS Package (.rustos)"
    }
}

impl Default for NativeAdapter {
    fn default() -> Self {
        Self::new()
    }
}
