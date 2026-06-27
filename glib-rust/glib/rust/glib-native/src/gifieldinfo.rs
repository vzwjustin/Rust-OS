//! `gifieldinfo` matching `girepository/gifieldinfo.h`.
//!
//! Field info: describes a struct/union field.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gitypes::{GIArgument, GIFieldInfoFlags};

/// Field info (mirrors `GIFieldInfo`).
#[derive(Debug, Clone, Default)]
pub struct FieldInfo {
    pub flags: GIFieldInfoFlags,
    pub size: usize,
    pub offset: usize,
}

impl FieldInfo {
    /// Creates a new field info.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the flags (mirrors `gi_field_info_get_flags`).
    pub fn flags(&self) -> GIFieldInfoFlags {
        self.flags
    }

    /// Returns the size (mirrors `gi_field_info_get_size`).
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns the offset (mirrors `gi_field_info_get_offset`).
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Gets a field value (mirrors `gi_field_info_get_field`).
    /// No-op in our no_std port.
    pub fn get_field(&self, _mem: *mut u8, _value: &mut GIArgument) -> bool {
        false
    }

    /// Sets a field value (mirrors `gi_field_info_set_field`).
    /// No-op in our no_std port.
    pub fn set_field(&self, _mem: *mut u8, _value: &GIArgument) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let fi = FieldInfo::new();
        assert_eq!(fi.flags(), GIFieldInfoFlags::NONE);
        assert_eq!(fi.size(), 0);
        assert_eq!(fi.offset(), 0);
    }

    #[test]
    fn test_custom() {
        let mut fi = FieldInfo::new();
        fi.flags = GIFieldInfoFlags::READABLE;
        fi.size = 4;
        fi.offset = 8;
        assert_eq!(fi.flags(), GIFieldInfoFlags::READABLE);
        assert_eq!(fi.size(), 4);
        assert_eq!(fi.offset(), 8);
    }
}
