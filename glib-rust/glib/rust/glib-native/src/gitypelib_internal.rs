//! `gitypelib_internal` matching `girepository/gitypelib-internal.h`.
//!
//! Internal typelib binary format structures and helpers.
//! Re-exports from the existing `gitypelib` module.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

pub use crate::gitypelib::Typelib;

use crate::prelude::*;
use alloc::string::String;

/// Typelib header magic (mirrors the typelib magic bytes).
pub const TYPELIB_MAGIC: &[u8; 8] = b"GOBJ-IR\0";

/// Typelib header version.
pub const TYPELIB_MAJOR_VERSION: u16 = 2;
pub const TYPELIB_MINOR_VERSION: u16 = 0;

/// Typelib header (mirrors `Header` in gitypelib-internal.h).
#[derive(Debug, Clone, Default)]
pub struct TypelibHeader {
    pub magic: [u8; 8],
    pub major_version: u16,
    pub minor_version: u16,
    pub n_entries: u16,
    pub n_local_entries: u16,
    pub n_attributes: u16,
    pub size: u32,
}

impl TypelibHeader {
    /// Creates a default header.
    pub fn new() -> Self {
        Self {
            magic: *TYPELIB_MAGIC,
            major_version: TYPELIB_MAJOR_VERSION,
            minor_version: TYPELIB_MINOR_VERSION,
            ..Default::default()
        }
    }

    /// Validates the magic bytes.
    pub fn is_valid(&self) -> bool {
        &self.magic == TYPELIB_MAGIC.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_new() {
        let h = TypelibHeader::new();
        assert!(h.is_valid());
        assert_eq!(h.major_version, 2);
        assert_eq!(h.minor_version, 0);
    }

    #[test]
    fn test_header_invalid() {
        let h = TypelibHeader::default();
        assert!(!h.is_valid());
    }
}
