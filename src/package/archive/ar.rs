//! AR archive format parser
//!
//! The ar archive format is used by .deb packages and static libraries.
//! Format:
//! - Global header: "!<arch>\n" (8 bytes)
//! - File entries: 60-byte header + file data
//!
//! Reference: https://en.wikipedia.org/wiki/Ar_(Unix)

use crate::package::{PackageError, PackageResult};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

const AR_MAGIC: &[u8] = b"!<arch>\n";
const AR_HEADER_SIZE: usize = 60;
const AR_FMAG: &[u8] = b"`\n";

/// AR archive member (file entry)
#[derive(Debug, Clone)]
pub struct ArMember {
    /// File name
    pub name: String,
    /// File modification timestamp
    pub timestamp: u64,
    /// Owner ID
    pub owner_id: u32,
    /// Group ID
    pub group_id: u32,
    /// File mode (permissions)
    pub mode: u32,
    /// File size in bytes
    pub size: usize,
    /// File data
    pub data: Vec<u8>,
}

/// AR archive parser
#[derive(Debug)]
pub struct ArArchive {
    /// Archive members
    members: Vec<ArMember>,
}

impl ArArchive {
    /// Parse an AR archive from bytes
    pub fn parse(data: &[u8]) -> PackageResult<Self> {
        if data.len() < AR_MAGIC.len() {
            return Err(PackageError::InvalidFormat(
                "File too small to be an AR archive".to_string(),
            ));
        }

        // Check magic number
        if &data[0..AR_MAGIC.len()] != AR_MAGIC {
            return Err(PackageError::InvalidFormat(
                "Invalid AR archive magic number".to_string(),
            ));
        }

        let mut members = Vec::new();
        let mut offset = AR_MAGIC.len();

        while offset + AR_HEADER_SIZE <= data.len() {
            // Parse header
            let header = &data[offset..offset + AR_HEADER_SIZE];

            // Check file magic at end of header
            if &header[58..60] != AR_FMAG {
                // Might be padding, skip
                break;
            }

            // Parse file name (16 bytes, space-padded)
            let name = Self::parse_string(&header[0..16])?;

            // Parse timestamp (12 bytes, decimal ASCII)
            let timestamp = Self::parse_decimal(&header[16..28])?;

            // Parse owner ID (6 bytes, decimal ASCII)
            let owner_id = Self::parse_decimal(&header[28..34])? as u32;

            // Parse group ID (6 bytes, decimal ASCII)
            let group_id = Self::parse_decimal(&header[34..40])? as u32;

            // Parse file mode (8 bytes, octal ASCII)
            let mode = Self::parse_octal(&header[40..48])? as u32;

            // Parse file size (10 bytes, decimal ASCII)
            let size = Self::parse_decimal(&header[48..58])? as usize;

            offset += AR_HEADER_SIZE;

            // Read file data
            if offset + size > data.len() {
                return Err(PackageError::InvalidFormat(
                    "AR archive member size exceeds file size".to_string(),
                ));
            }

            let file_data = data[offset..offset + size].to_vec();
            offset += size;

            // AR archive entries are 2-byte aligned
            if offset % 2 == 1 {
                offset += 1;
            }

            members.push(ArMember {
                name,
                timestamp,
                owner_id,
                group_id,
                mode,
                size,
                data: file_data,
            });
        }

        Ok(ArArchive { members })
    }

    /// Find a member by name
    pub fn find_member(&self, name: &str) -> Option<&[u8]> {
        for member in &self.members {
            if member.name == name || member.name.starts_with(name) {
                return Some(&member.data);
            }
        }
        None
    }

    /// Get all members
    pub fn members(&self) -> &[ArMember] {
        &self.members
    }

    /// Parse a space-padded string
    fn parse_string(data: &[u8]) -> PackageResult<String> {
        let s = core::str::from_utf8(data)
            .map_err(|_| PackageError::InvalidFormat("Invalid UTF-8 in AR header".to_string()))?;
        Ok(s.trim().to_string())
    }

    /// Parse a decimal number from ASCII
    fn parse_decimal(data: &[u8]) -> PackageResult<u64> {
        let s = core::str::from_utf8(data)
            .map_err(|_| PackageError::InvalidFormat("Invalid UTF-8 in AR header".to_string()))?;
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Ok(0);
        }
        trimmed.parse::<u64>().map_err(|_| {
            PackageError::InvalidFormat("Invalid decimal number in AR header".to_string())
        })
    }

    /// Parse an octal number from ASCII
    fn parse_octal(data: &[u8]) -> PackageResult<u64> {
        let s = core::str::from_utf8(data)
            .map_err(|_| PackageError::InvalidFormat("Invalid UTF-8 in AR header".to_string()))?;
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Ok(0);
        }
        u64::from_str_radix(trimmed, 8).map_err(|_| {
            PackageError::InvalidFormat("Invalid octal number in AR header".to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn test_ar_magic_validation() {
        assert_eq!(&b"!<arch>\n"[..], b"!<arch>\n");
        assert_ne!(&b"!<invalid"[..8], b"!<arch>\n");
    }
}
