//! Package adapter implementations for different package formats

use crate::package::{ExtractedPackage, PackageMetadata, PackageResult};

/// Trait for package format adapters
pub trait PackageAdapter {
    /// Extract a package from raw bytes
    fn extract(&self, data: &[u8]) -> PackageResult<ExtractedPackage>;

    /// Parse package metadata without full extraction
    fn parse_metadata(&self, data: &[u8]) -> PackageResult<PackageMetadata>;

    /// Validate package format
    fn validate(&self, data: &[u8]) -> PackageResult<bool>;

    /// Get the package format name
    fn format_name(&self) -> &str;
}

pub mod apk;
pub mod deb;
pub mod native;
pub mod rpm;

pub use apk::ApkAdapter;
pub use deb::DebAdapter;
pub use native::NativeAdapter;
pub use rpm::RpmAdapter;
