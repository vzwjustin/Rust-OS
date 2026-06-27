//! `giflagsinfo` matching `girepository/giflagsinfo.h`.
//!
//! Flags info: describes a flags (bitfield) type.
//! Extends `EnumInfo` since flags are enums with bitfield semantics.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gienuminfo::EnumInfo;
use alloc::sync::Arc;

/// Flags info (mirrors `GIFlagsInfo`).
#[derive(Debug, Default)]
pub struct FlagsInfo {
    pub enum_info: Option<Arc<EnumInfo>>,
}

impl FlagsInfo {
    /// Creates a new flags info.
    pub fn new() -> Self {
        Self { enum_info: None }
    }

    /// Creates a new flags info with the given values.
    pub fn with_values(
        name: impl Into<alloc::string::String>,
        namespace: impl Into<alloc::string::String>,
        values: &[(&str, i64, &str)],
    ) -> Self {
        Self {
            enum_info: Some(EnumInfo::new(name, namespace, values)),
        }
    }

    /// Delegates to the inner `EnumInfo`.
    pub fn enum_info(&self) -> Option<&EnumInfo> {
        self.enum_info.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let fi = FlagsInfo::new();
        assert!(fi.enum_info().is_none());
    }

    #[test]
    fn test_with_values() {
        let fi = FlagsInfo::with_values("MyFlags", "Test", &[("NONE", 0, "none")]);
        assert_eq!(fi.enum_info().unwrap().n_values(), 1);
    }
}
