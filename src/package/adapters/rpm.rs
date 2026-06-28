//! RPM package adapter
//!
//! Adapter for the RPM package format used by Fedora, RHEL, CentOS, openSUSE.
//!
//! An RPM file is laid out as:
//!   1. **Lead** (96 bytes): magic `0xEDABEEDB`, version, name, arch, os.
//!   2. **Signature header**: RPM header structure (used for integrity).
//!   3. 8-byte alignment padding.
//!   4. **Payload header**: RPM header structure carrying the metadata tags
//!      (`RPMTAG_NAME`, `RPMTAG_VERSION`, `RPMTAG_RELEASE`, ...).
//!   5. **Payload**: a `cpio` archive (newc format), usually compressed with
//!      gzip/xz/bzip2/zstd.
//!
//! The RPM header structure is:
//!   - magic `0x8E 0xAD 0xE8`, version byte `0x01`, 4 reserved bytes
//!   - `nindex` (u32 BE): number of index entries
//!   - `hsize` (u32 BE): length of the data store
//!   - `nindex` index entries (16 bytes each): tag, type, offset, count (u32 BE)
//!   - `hsize` bytes of data
//!
//! This adapter parses the lead and payload header to recover metadata, and
//! decompresses + parses the cpio payload to extract files.

use crate::package::adapters::PackageAdapter;
use crate::package::compression::{decompress, CompressionFormat};
use crate::package::{ExtractedPackage, PackageError, PackageMetadata, PackageResult};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

const RPM_LEAD_MAGIC: u32 = 0xEDABEEDB;
const RPM_LEAD_SIZE: usize = 96;
const RPM_HEADER_MAGIC: [u8; 3] = [0x8e, 0xad, 0xe8];
const RPM_HEADER_VERSION: u8 = 1;

// RPM header entry types.
const RPM_TYPE_INT16: u32 = 3;
const RPM_TYPE_INT32: u32 = 4;
const RPM_TYPE_INT64: u32 = 5;
const RPM_TYPE_STRING: u32 = 6;
const RPM_TYPE_STRING_ARRAY: u32 = 8;
const RPM_TYPE_I18NSTRING: u32 = 9;

// RPM metadata tags (see `rpmtag.h`).
const RPMTAG_NAME: u32 = 1000;
const RPMTAG_VERSION: u32 = 1001;
const RPMTAG_RELEASE: u32 = 1002;
const RPMTAG_SUMMARY: u32 = 1004;
const RPMTAG_DESCRIPTION: u32 = 1005;
const RPMTAG_SIZE: u32 = 1009;
const RPMTAG_URL: u32 = 1020;
const RPMTAG_ARCH: u32 = 1022;
const RPMTAG_REQUIRENAME: u32 = 1049;
const RPMTAG_VENDOR: u32 = 1011;

/// RPM package adapter
pub struct RpmAdapter;

impl RpmAdapter {
    /// Create a new RPM package adapter
    pub fn new() -> Self {
        RpmAdapter
    }

    /// Read a big-endian u32 from `data` at `offset`.
    fn read_u32_be(data: &[u8], offset: usize) -> PackageResult<u32> {
        if offset + 4 > data.len() {
            return Err(PackageError::InvalidFormat(
                "RPM data truncated reading u32".to_string(),
            ));
        }
        Ok(u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]))
    }

    /// Parse the 96-byte RPM lead and return the architecture name embedded in it.
    fn parse_lead(data: &[u8]) -> PackageResult<()> {
        if data.len() < RPM_LEAD_SIZE {
            return Err(PackageError::InvalidFormat(
                "RPM file too small for lead".to_string(),
            ));
        }

        let magic = Self::read_u32_be(data, 0)?;
        if magic != RPM_LEAD_MAGIC {
            return Err(PackageError::InvalidFormat(
                "Invalid RPM lead magic number".to_string(),
            ));
        }

        Ok(())
    }

    /// Parse an RPM header structure beginning at `offset`.
    ///
    /// Returns the parsed header and the total number of bytes consumed
    /// (header preamble + index + data store).
    fn parse_header(data: &[u8], offset: usize) -> PackageResult<(RpmHeader, usize)> {
        if offset + 16 > data.len() {
            return Err(PackageError::InvalidFormat(
                "RPM header truncated".to_string(),
            ));
        }

        if &data[offset..offset + 3] != RPM_HEADER_MAGIC {
            return Err(PackageError::InvalidFormat(
                "Invalid RPM header magic".to_string(),
            ));
        }

        if data[offset + 3] != RPM_HEADER_VERSION {
            return Err(PackageError::InvalidFormat(format!(
                "Unsupported RPM header version: {}",
                data[offset + 3]
            )));
        }

        let nindex = Self::read_u32_be(data, offset + 8)? as usize;
        let hsize = Self::read_u32_be(data, offset + 12)? as usize;

        let index_start = offset + 16;
        let index_end = index_start + nindex * 16;
        if index_end > data.len() {
            return Err(PackageError::InvalidFormat(
                "RPM header index truncated".to_string(),
            ));
        }

        let data_start = index_end;
        let data_end = data_start + hsize;
        if data_end > data.len() {
            return Err(PackageError::InvalidFormat(
                "RPM header data store truncated".to_string(),
            ));
        }

        let mut entries = Vec::with_capacity(nindex);
        for i in 0..nindex {
            let base = index_start + i * 16;
            let tag = Self::read_u32_be(data, base)?;
            let typ = Self::read_u32_be(data, base + 4)?;
            let entry_offset = Self::read_u32_be(data, base + 8)? as usize;
            let count = Self::read_u32_be(data, base + 12)? as usize;
            entries.push(RpmIndexEntry {
                tag,
                typ,
                offset: entry_offset,
                count,
            });
        }

        let header = RpmHeader {
            entries,
            data: data[data_start..data_end].to_vec(),
        };

        Ok((header, data_end - offset))
    }

    /// Extract a single string value for `tag` from the header.
    fn get_string(header: &RpmHeader, tag: u32) -> Option<String> {
        let entry = header.entries.iter().find(|e| e.tag == tag)?;
        if entry.typ != RPM_TYPE_STRING && entry.typ != RPM_TYPE_I18NSTRING {
            return None;
        }
        let start = entry.offset;
        if start >= header.data.len() {
            return None;
        }
        let end = header.data[start..]
            .iter()
            .position(|&b| b == 0)
            .map(|p| start + p)
            .unwrap_or(header.data.len());
        core::str::from_utf8(&header.data[start..end])
            .ok()
            .map(|s| s.to_string())
    }

    /// Extract a string array for `tag` (STRING_ARRAY / I18NSTRING with count>1).
    fn get_string_array(header: &RpmHeader, tag: u32) -> Vec<String> {
        let mut out = Vec::new();
        for entry in header.entries.iter().filter(|e| e.tag == tag) {
            if entry.typ != RPM_TYPE_STRING_ARRAY
                && entry.typ != RPM_TYPE_I18NSTRING
                && entry.typ != RPM_TYPE_STRING
            {
                continue;
            }
            let mut cursor = entry.offset;
            for _ in 0..entry.count {
                if cursor >= header.data.len() {
                    break;
                }
                let end = header.data[cursor..]
                    .iter()
                    .position(|&b| b == 0)
                    .map(|p| cursor + p)
                    .unwrap_or(header.data.len());
                if let Ok(s) = core::str::from_utf8(&header.data[cursor..end]) {
                    out.push(s.to_string());
                }
                cursor = end + 1;
            }
        }
        out
    }

    /// Extract a u64 integer for `tag` (INT32/INT64).
    fn get_u64(header: &RpmHeader, tag: u32) -> Option<u64> {
        let entry = header.entries.iter().find(|e| e.tag == tag)?;
        let start = entry.offset;
        match entry.typ {
            RPM_TYPE_INT16 => {
                if start + 2 > header.data.len() {
                    return None;
                }
                Some(u16::from_be_bytes([header.data[start], header.data[start + 1]]) as u64)
            }
            RPM_TYPE_INT32 => {
                if start + 4 > header.data.len() {
                    return None;
                }
                Some(u32::from_be_bytes([
                    header.data[start],
                    header.data[start + 1],
                    header.data[start + 2],
                    header.data[start + 3],
                ]) as u64)
            }
            RPM_TYPE_INT64 => {
                if start + 8 > header.data.len() {
                    return None;
                }
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&header.data[start..start + 8]);
                Some(u64::from_be_bytes(bytes))
            }
            _ => None,
        }
    }

    /// Build `PackageMetadata` from a parsed payload header.
    fn metadata_from_header(header: &RpmHeader) -> PackageResult<PackageMetadata> {
        let name = Self::get_string(header, RPMTAG_NAME).unwrap_or_default();
        let version = Self::get_string(header, RPMTAG_VERSION).unwrap_or_default();
        let release = Self::get_string(header, RPMTAG_RELEASE);
        let arch = Self::get_string(header, RPMTAG_ARCH).unwrap_or_else(|| "noarch".to_string());
        let description = Self::get_string(header, RPMTAG_DESCRIPTION)
            .unwrap_or_else(|| Self::get_string(header, RPMTAG_SUMMARY).unwrap_or_default());
        let homepage = Self::get_string(header, RPMTAG_URL);
        let maintainer = Self::get_string(header, RPMTAG_VENDOR);
        let size = Self::get_u64(header, RPMTAG_SIZE).unwrap_or(0);
        let dependencies = Self::get_string_array(header, RPMTAG_REQUIRENAME)
            .into_iter()
            .filter(|d| !d.is_empty() && !d.starts_with("rpmlib("))
            .collect();

        if name.is_empty() || version.is_empty() {
            return Err(PackageError::InvalidFormat(
                "RPM header missing name/version".to_string(),
            ));
        }

        // Combine version and release into the version field (e.g. "1.2.3-1.el9").
        let full_version = match release {
            Some(rel) if !rel.is_empty() => format!("{}-{}", version, rel),
            _ => version,
        };

        let mut metadata = PackageMetadata::new(name, full_version, arch);
        metadata.description = description;
        metadata.maintainer = maintainer;
        metadata.homepage = homepage;
        metadata.dependencies = dependencies;
        metadata.size = size;
        // RPM doesn't carry an installed-size tag; approximate from payload size.
        metadata.installed_size = size;

        Ok(metadata)
    }

    /// Locate the payload (compressed cpio) and return its raw bytes plus the
    /// detected compression format.
    fn locate_payload(data: &[u8]) -> PackageResult<(&[u8], CompressionFormat)> {
        // Skip the lead, then the signature header, then align to 8 bytes,
        // then the payload header; everything after the payload header is the
        // compressed cpio stream.
        Self::parse_lead(data)?;
        let (_sig_header, sig_end) = Self::parse_header(data, RPM_LEAD_SIZE)?;

        // Signature header is aligned to an 8-byte boundary before the next
        // header begins.
        let after_sig = RPM_LEAD_SIZE + sig_end;
        let aligned = (after_sig + 7) & !7;

        let (_payload_header, payload_end) = Self::parse_header(data, aligned)?;
        let payload = &data[aligned + payload_end..];
        let format = CompressionFormat::detect(payload);
        Ok((payload, format))
    }

    /// Parse a cpio "newc" archive into a map of path -> data.
    ///
    /// The newc format uses a 110-byte ASCII-hex header per entry followed by
    // the file name and the file data, each padded to a 4-byte boundary.
    fn parse_cpio(data: &[u8]) -> PackageResult<Vec<(String, Vec<u8>)>> {
        let mut files = Vec::new();
        let mut cursor = 0usize;

        while cursor + 110 <= data.len() {
            let header = &data[cursor..cursor + 110];
            let magic = &header[0..6];
            if magic != b"070701" && magic != b"070702" {
                // Not a newc entry; stop parsing.
                break;
            }

            let namesize = Self::parse_hex(&header[94..102])? as usize;
            let filesize = Self::parse_hex(&header[54..62])? as usize;

            cursor += 110;
            if cursor + namesize > data.len() {
                return Err(PackageError::InvalidFormat(
                    "CPIO entry name truncated".to_string(),
                ));
            }

            let name_end = cursor + namesize;
            // Strip the trailing NUL.
            let name_bytes = &data[cursor..name_end];
            let name_len = name_bytes
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(name_bytes.len());
            let name = core::str::from_utf8(&name_bytes[..name_len])
                .map_err(|_| PackageError::InvalidFormat("Invalid UTF-8 in cpio name".to_string()))?
                .to_string();

            // Advance past the name, padded to a 4-byte boundary.
            cursor = (name_end + 3) & !3;

            if name == "TRAILER!!!" {
                break;
            }

            if cursor + filesize > data.len() {
                return Err(PackageError::InvalidFormat(
                    "CPIO entry data truncated".to_string(),
                ));
            }

            let file_data = data[cursor..cursor + filesize].to_vec();
            cursor = (cursor + filesize + 3) & !3;

            // Skip non-regular entries (directories, symlinks) which have no
            // data payload but still consume a header.
            if !name.is_empty() && !name.ends_with('/') {
                files.push((name, file_data));
            }
        }

        Ok(files)
    }

    /// Parse an 8-character ASCII-hex value into a u64.
    fn parse_hex(bytes: &[u8]) -> PackageResult<u64> {
        let s = core::str::from_utf8(bytes)
            .map_err(|_| PackageError::InvalidFormat("Invalid hex in cpio header".to_string()))?;
        u64::from_str_radix(s.trim_end_matches('\0').trim(), 16).map_err(|_| {
            PackageError::InvalidFormat("Invalid hex digits in cpio header".to_string())
        })
    }
}

/// Parsed RPM header (index entries + data store).
struct RpmHeader {
    entries: Vec<RpmIndexEntry>,
    data: Vec<u8>,
}

struct RpmIndexEntry {
    tag: u32,
    typ: u32,
    offset: usize,
    count: usize,
}

impl PackageAdapter for RpmAdapter {
    fn extract(&self, data: &[u8]) -> PackageResult<ExtractedPackage> {
        if !self.validate(data)? {
            return Err(PackageError::InvalidFormat(
                "RPM file format validation failed".to_string(),
            ));
        }

        // Parse the payload header for metadata.
        let metadata = self.parse_metadata(data)?;
        let mut package = ExtractedPackage::new(metadata);

        // Locate and decompress the cpio payload.
        let (payload, _format) = Self::locate_payload(data)?;
        let decompressed = decompress(payload)?;
        let files = Self::parse_cpio(&decompressed)?;
        for (path, file_data) in files {
            package.add_file(path, file_data);
        }

        Ok(package)
    }

    fn parse_metadata(&self, data: &[u8]) -> PackageResult<PackageMetadata> {
        if !self.validate(data)? {
            return Err(PackageError::InvalidFormat(
                "RPM file format validation failed".to_string(),
            ));
        }

        Self::parse_lead(data)?;
        let (_sig_header, sig_end) = Self::parse_header(data, RPM_LEAD_SIZE)?;
        let after_sig = RPM_LEAD_SIZE + sig_end;
        let aligned = (after_sig + 7) & !7;
        let (payload_header, _) = Self::parse_header(data, aligned)?;
        Self::metadata_from_header(&payload_header)
    }

    fn validate(&self, data: &[u8]) -> PackageResult<bool> {
        // RPM files start with magic number 0xEDABEEDB (lead signature).
        if data.len() < 4 {
            return Ok(false);
        }

        let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        Ok(magic == RPM_LEAD_MAGIC)
    }

    fn format_name(&self) -> &str {
        "RPM Package (.rpm)"
    }
}

impl Default for RpmAdapter {
    fn default() -> Self {
        Self::new()
    }
}
