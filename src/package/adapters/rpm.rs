//! RPM package adapter
//!
//! Experimental adapter for RPM package format used by Fedora, RHEL, CentOS.
//! RPM packages use a binary format with headers and compressed payload.

use crate::package::adapters::PackageAdapter;
use crate::package::{ExtractedPackage, PackageError, PackageMetadata, PackageResult};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// RPM package adapter
pub struct RpmAdapter;

impl RpmAdapter {
    /// Create a new RPM package adapter
    pub fn new() -> Self {
        RpmAdapter
    }
}

impl PackageAdapter for RpmAdapter {
    fn extract(&self, _data: &[u8]) -> PackageResult<ExtractedPackage> {
        Err(PackageError::NotImplemented(
            "RPM extraction not yet implemented".to_string(),
        ))
    }

    fn parse_metadata(&self, _data: &[u8]) -> PackageResult<PackageMetadata> {
        Err(PackageError::NotImplemented(
            "RPM metadata parsing not yet implemented".to_string(),
        ))
    }

    fn validate(&self, data: &[u8]) -> PackageResult<bool> {
        // RPM files start with magic number 0xEDABEEDB (lead signature)
        if data.len() < 4 {
            return Ok(false);
        }

        let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        Ok(magic == 0xEDABEEDB)
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
