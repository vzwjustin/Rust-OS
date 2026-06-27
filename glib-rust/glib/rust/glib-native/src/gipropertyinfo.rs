//! `gipropertyinfo` matching `girepository/gipropertyinfo.h`.
//!
//! Property info: describes a GObject property.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gifunctioninfo::FunctionInfo;
use crate::gitypes::GITransfer;

/// Property info (mirrors `GIPropertyInfo`).
#[derive(Debug, Clone, Default)]
pub struct PropertyInfo {
    pub flags: u32,
    pub ownership_transfer: GITransfer,
    pub setter: Option<FunctionInfo>,
    pub getter: Option<FunctionInfo>,
}

impl PropertyInfo {
    /// Creates a new property info.
    pub fn new() -> Self {
        Self {
            flags: 0,
            ownership_transfer: GITransfer::Nothing,
            setter: None,
            getter: None,
        }
    }

    /// Returns the flags (mirrors `gi_property_info_get_flags`).
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns ownership transfer (mirrors `gi_property_info_get_ownership_transfer`).
    pub fn ownership_transfer(&self) -> GITransfer {
        self.ownership_transfer
    }

    /// Returns the setter (mirrors `gi_property_info_get_setter`).
    pub fn setter(&self) -> Option<&FunctionInfo> {
        self.setter.as_ref()
    }

    /// Returns the getter (mirrors `gi_property_info_get_getter`).
    pub fn getter(&self) -> Option<&FunctionInfo> {
        self.getter.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let pi = PropertyInfo::new();
        assert_eq!(pi.flags(), 0);
        assert_eq!(pi.ownership_transfer(), GITransfer::Nothing);
        assert!(pi.setter().is_none());
        assert!(pi.getter().is_none());
    }
}
