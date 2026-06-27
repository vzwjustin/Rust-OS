//! Native RustOS package adapter
//!
//! This adapter handles native RustOS package format, optimized for the kernel.

use crate::package::adapters::PackageAdapter;
use crate::package::{ExtractedPackage, PackageError, PackageMetadata, PackageResult};
use alloc::string::ToString;

/// Native RustOS package adapter
pub struct NativeAdapter;

impl NativeAdapter {
    /// Create a new native package adapter
    pub fn new() -> Self {
        NativeAdapter
    }
}

impl PackageAdapter for NativeAdapter {
    fn extract(&self, _data: &[u8]) -> PackageResult<ExtractedPackage> {
        Err(PackageError::NotImplemented(
            "Native RustOS package extraction not yet implemented".to_string(),
        ))
    }

    fn parse_metadata(&self, _data: &[u8]) -> PackageResult<PackageMetadata> {
        Err(PackageError::NotImplemented(
            "Native RustOS package metadata parsing not yet implemented".to_string(),
        ))
    }

    fn validate(&self, data: &[u8]) -> PackageResult<bool> {
        // Native RustOS packages use custom magic number "RUSTOS\0\0"
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
