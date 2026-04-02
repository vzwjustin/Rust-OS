//! Package adapter implementations for different package formats

use crate::package::{PackageResult, PackageMetadata, ExtractedPackage};

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

pub mod deb;
pub mod rpm;
pub mod apk;
pub mod native;

pub use deb::DebAdapter;
pub use rpm::RpmAdapter;
pub use apk::ApkAdapter;
pub use native::NativeAdapter;
