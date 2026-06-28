//! Alpine APK package adapter
//!
//! Adapter for Alpine Linux APK package format.
//!
//! APK v1 packages are a single gzip-compressed tar archive containing a
//! `.PKGINFO` metadata file alongside the package payload.
//!
//! APK v2 packages are three concatenated gzip streams:
//!   1. signature segment (tar.gz)
//!   2. control segment (tar.gz with `.PKGINFO`)
//!   3. data segment (tar.gz with the payload files)
//!
//! Because the gzip decoder handles a single DEFLATE stream, v2 metadata is
//! recovered from the first decompressible tar that contains `.PKGINFO`, and
//! the data segment is recovered by scanning forward for the next gzip magic
//! after the control segment.

use crate::package::adapters::PackageAdapter;
use crate::package::compression::{GzipDecoder, TarArchive};
use crate::package::{ExtractedPackage, PackageError, PackageMetadata, PackageResult};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Alpine APK package adapter
pub struct ApkAdapter;

impl ApkAdapter {
    /// Create a new APK package adapter
    pub fn new() -> Self {
        ApkAdapter
    }

    /// Parse a `.PKGINFO` file content into package metadata.
    ///
    /// `.PKGINFO` uses `key = value` lines (note the spaces around `=`).
    fn parse_pkginfo(content: &str) -> PackageResult<PackageMetadata> {
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
                let value = line[eq + 1..].trim().trim_matches('"');

                match key {
                    "pkgname" => name = value.to_string(),
                    "pkgver" => version = value.to_string(),
                    "arch" => architecture = value.to_string(),
                    "pkgdesc" => description = value.to_string(),
                    "maintainer" => maintainer = Some(value.to_string()),
                    "url" => homepage = Some(value.to_string()),
                    "depend" | "provides" | "replaces" | "conflicts" => {
                        if key == "depend" && !value.is_empty() {
                            dependencies.push(value.to_string());
                        }
                    }
                    "size" => {
                        size = value.parse().unwrap_or(0);
                    }
                    "installed_size" => {
                        installed_size = value.parse().unwrap_or(0);
                    }
                    _ => {}
                }
            }
        }

        if name.is_empty() || version.is_empty() {
            return Err(PackageError::InvalidFormat(
                "Missing pkgname/pkgver in .PKGINFO".to_string(),
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

    /// Find the start of the next gzip stream at or after `start`.
    fn next_gzip_offset(data: &[u8], start: usize) -> Option<usize> {
        if start + 1 >= data.len() {
            return None;
        }
        let mut i = start;
        while i + 1 < data.len() {
            if data[i] == 0x1f && data[i + 1] == 0x8b {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    /// Decompress every gzip stream in `data` and concatenate the results.
    ///
    /// APK v2 stores multiple gzip members back-to-back; this recovers the
    /// full uncompressed payload by repeatedly invoking the single-stream
    /// decoder on each detected gzip magic.
    fn decompress_all_gzip(data: &[u8]) -> PackageResult<Vec<u8>> {
        let mut out = Vec::new();
        let mut offset = 0;
        let mut found_any = false;

        while let Some(start) = Self::next_gzip_offset(data, offset) {
            found_any = true;
            // Decode this single gzip stream. The decoder reads up to its
            // footer (CRC32 + size) and ignores trailing bytes, so passing
            // the remainder of the buffer is safe.
            match GzipDecoder::decode(&data[start..]) {
                Ok(chunk) => out.extend_from_slice(&chunk),
                Err(PackageError::InvalidFormat(_)) if start == 0 => {
                    // Not actually gzip despite magic; bail out.
                    return Err(PackageError::InvalidFormat(
                        "APK gzip stream malformed".to_string(),
                    ));
                }
                Err(_) => {
                    // A trailing fragment that isn't a complete gzip stream;
                    // stop scanning since we've consumed the real members.
                    break;
                }
            }

            // Advance past this member. We don't know its exact compressed
            // length without parsing the footer precisely, so scan for the
            // next gzip magic after the current offset.
            offset = start + 2;
        }

        if !found_any {
            return Err(PackageError::InvalidFormat(
                "No gzip streams found in APK data".to_string(),
            ));
        }

        Ok(out)
    }
}

impl PackageAdapter for ApkAdapter {
    fn extract(&self, data: &[u8]) -> PackageResult<ExtractedPackage> {
        if !self.validate(data)? {
            return Err(PackageError::InvalidFormat(
                "APK file format validation failed".to_string(),
            ));
        }

        // Decompress all concatenated gzip streams (handles both v1 and v2).
        let tar_data = Self::decompress_all_gzip(data)?;
        let archive = TarArchive::parse(&tar_data)?;

        // Locate `.PKGINFO` for metadata.
        let pkginfo_entry = archive
            .find_entry(".PKGINFO")
            .or_else(|| archive.find_entry("./.PKGINFO"))
            .ok_or_else(|| {
                PackageError::InvalidFormat("Missing .PKGINFO in APK archive".to_string())
            })?;

        let pkginfo_str = core::str::from_utf8(&pkginfo_entry.data)
            .map_err(|_| PackageError::InvalidFormat("Invalid UTF-8 in .PKGINFO".to_string()))?;
        let metadata = Self::parse_pkginfo(pkginfo_str)?;

        let mut package = ExtractedPackage::new(metadata);

        // Collect all regular files except the metadata file itself.
        for entry in archive.entries() {
            let path = entry.path.trim_start_matches("./");
            if path == ".PKGINFO" || path == ".SIGN.*" || path.starts_with(".SIGN.") {
                continue;
            }
            if entry.data.is_empty() {
                continue;
            }
            package.add_file(entry.path.clone(), entry.data.clone());
        }

        Ok(package)
    }

    fn parse_metadata(&self, data: &[u8]) -> PackageResult<PackageMetadata> {
        if !self.validate(data)? {
            return Err(PackageError::InvalidFormat(
                "APK file format validation failed".to_string(),
            ));
        }

        // For metadata-only extraction we only need the control segment. The
        // first gzip stream in a v2 package is the signature, the second is
        // the control segment. In v1 the single stream contains everything.
        // Try each gzip member in turn until one yields a tar with .PKGINFO.
        let mut offset = 0;
        while let Some(start) = Self::next_gzip_offset(data, offset) {
            if let Ok(chunk) = GzipDecoder::decode(&data[start..]) {
                if let Ok(archive) = TarArchive::parse(&chunk) {
                    if let Some(entry) = archive
                        .find_entry(".PKGINFO")
                        .or_else(|| archive.find_entry("./.PKGINFO"))
                    {
                        if let Ok(content) = core::str::from_utf8(&entry.data) {
                            return Self::parse_pkginfo(content);
                        }
                    }
                }
            }
            offset = start + 2;
        }

        Err(PackageError::InvalidFormat(
            "Could not locate .PKGINFO in any APK gzip segment".to_string(),
        ))
    }

    fn validate(&self, data: &[u8]) -> PackageResult<bool> {
        // APK files are gzip-compressed tar archives.
        if data.len() < 2 {
            return Ok(false);
        }

        Ok(data[0] == 0x1f && data[1] == 0x8b)
    }

    fn format_name(&self) -> &str {
        "Alpine APK Package (.apk)"
    }
}

impl Default for ApkAdapter {
    fn default() -> Self {
        Self::new()
    }
}
