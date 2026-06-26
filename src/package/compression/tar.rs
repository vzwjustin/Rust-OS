//! TAR archive extraction
//!
//! Basic TAR archive parsing for package extraction.
//! Supports POSIX ustar format used by most packages.

use crate::package::{PackageError, PackageResult};
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::str;

const TAR_BLOCK_SIZE: usize = 512;
const TAR_NAME_SIZE: usize = 100;
const TAR_MODE_OFFSET: usize = 100;
const TAR_SIZE_OFFSET: usize = 124;
const TAR_MAGIC_OFFSET: usize = 257;
const TAR_USTAR_MAGIC: &[u8] = b"ustar";

/// TAR file entry types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TarEntryType {
    /// Regular file
    File,
    /// Hard link
    HardLink,
    /// Symbolic link
    SymLink,
    /// Character device
    CharDevice,
    /// Block device
    BlockDevice,
    /// Directory
    Directory,
    /// FIFO
    Fifo,
}

impl TarEntryType {
    fn from_byte(b: u8) -> Self {
        match b {
            b'0' | 0 => TarEntryType::File,
            b'1' => TarEntryType::HardLink,
            b'2' => TarEntryType::SymLink,
            b'3' => TarEntryType::CharDevice,
            b'4' => TarEntryType::BlockDevice,
            b'5' => TarEntryType::Directory,
            b'6' => TarEntryType::Fifo,
            _ => TarEntryType::File, // Default to file
        }
    }
}

/// TAR archive entry
#[derive(Debug, Clone)]
pub struct TarEntry {
    /// File path
    pub path: String,
    /// File mode (permissions)
    pub mode: u32,
    /// File size
    pub size: usize,
    /// Entry type
    pub entry_type: TarEntryType,
    /// File data
    pub data: Vec<u8>,
}

/// TAR archive parser
pub struct TarArchive {
    entries: Vec<TarEntry>,
}

impl TarArchive {
    /// Parse a TAR archive from bytes
    pub fn parse(data: &[u8]) -> PackageResult<Self> {
        let mut entries = Vec::new();
        let mut offset = 0;

        while offset + TAR_BLOCK_SIZE <= data.len() {
            let header = &data[offset..offset + TAR_BLOCK_SIZE];

            // Check for end of archive (two consecutive zero blocks)
            if Self::is_zero_block(header) {
                break;
            }

            // Validate magic number for ustar format
            if offset + TAR_MAGIC_OFFSET + 5 <= data.len() {
                let magic = &data[offset + TAR_MAGIC_OFFSET..offset + TAR_MAGIC_OFFSET + 5];
                if magic != TAR_USTAR_MAGIC {
                    // Not a ustar archive, might be old-style tar
                    // Try to parse anyway if it looks like a header
                }
            }

            // Parse file name
            let name_bytes = &header[0..TAR_NAME_SIZE];
            let name = Self::parse_null_terminated_string(name_bytes)?;

            // Parse file mode (octal)
            let mode = Self::parse_octal(&header[TAR_MODE_OFFSET..TAR_MODE_OFFSET + 8])? as u32;

            // Parse file size (octal)
            let size = Self::parse_octal(&header[TAR_SIZE_OFFSET..TAR_SIZE_OFFSET + 12])? as usize;

            // Parse entry type
            let entry_type = TarEntryType::from_byte(header[156]);

            offset += TAR_BLOCK_SIZE;

            // Read file data
            let mut file_data = Vec::new();
            if size > 0 {
                if offset + size > data.len() {
                    return Err(PackageError::InvalidFormat(
                        "TAR entry size exceeds archive size".into(),
                    ));
                }

                file_data.extend_from_slice(&data[offset..offset + size]);

                // TAR blocks are 512-byte aligned
                let padded_size = (size + TAR_BLOCK_SIZE - 1) / TAR_BLOCK_SIZE * TAR_BLOCK_SIZE;
                offset += padded_size;
            }

            entries.push(TarEntry {
                path: name,
                mode,
                size,
                entry_type,
                data: file_data,
            });
        }

        Ok(TarArchive { entries })
    }

    /// Find an entry by path
    pub fn find_entry(&self, path: &str) -> Option<&TarEntry> {
        self.entries
            .iter()
            .find(|e| e.path == path || e.path.ends_with(path))
    }

    /// Get all entries
    pub fn entries(&self) -> &[TarEntry] {
        &self.entries
    }

    /// Extract all files to a map
    pub fn extract_all(&self) -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::new();
        for entry in &self.entries {
            if entry.entry_type == TarEntryType::File {
                files.insert(entry.path.clone(), entry.data.clone());
            }
        }
        files
    }

    /// Check if a block is all zeros
    fn is_zero_block(block: &[u8]) -> bool {
        block.iter().all(|&b| b == 0)
    }

    /// Parse a null-terminated string from bytes
    fn parse_null_terminated_string(data: &[u8]) -> PackageResult<String> {
        let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
        str::from_utf8(&data[0..end])
            .map(|s| s.to_string())
            .map_err(|_| PackageError::InvalidFormat("Invalid UTF-8 in TAR header".into()))
    }

    /// Parse an octal number from ASCII bytes
    fn parse_octal(data: &[u8]) -> PackageResult<u64> {
        let s = str::from_utf8(data)
            .map_err(|_| PackageError::InvalidFormat("Invalid UTF-8 in TAR header".into()))?;

        let trimmed = s.trim_end_matches('\0').trim();
        if trimmed.is_empty() {
            return Ok(0);
        }

        u64::from_str_radix(trimmed, 8).map_err(|_| {
            PackageError::InvalidFormat(format!("Invalid octal number in TAR header: {}", trimmed))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_tar_entry_type() {
        assert_eq!(TarEntryType::from_byte(b'0'), TarEntryType::File);
        assert_eq!(TarEntryType::from_byte(b'5'), TarEntryType::Directory);
        assert_eq!(TarEntryType::from_byte(b'2'), TarEntryType::SymLink);
    }
}
