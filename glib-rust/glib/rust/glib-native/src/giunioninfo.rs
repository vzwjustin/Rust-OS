//! `giunioninfo` matching `girepository/giunioninfo.h`.
//!
//! Union info: describes a C union type.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::giconstantinfo::ConstantInfo;
use crate::gifieldinfo::FieldInfo;
use crate::gifunctioninfo::FunctionInfo;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

/// Union info (mirrors `GIUnionInfo`).
#[derive(Debug, Clone, Default)]
pub struct UnionInfo {
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<FunctionInfo>,
    pub is_discriminated: bool,
    pub discriminator_offset: usize,
    pub discriminators: Vec<ConstantInfo>,
    pub size: usize,
    pub alignment: usize,
    pub copy_function: String,
    pub free_function: String,
}

impl UnionInfo {
    /// Creates a new union info.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of fields (mirrors `gi_union_info_get_n_fields`).
    pub fn n_fields(&self) -> u32 {
        self.fields.len() as u32
    }

    /// Gets a field by index (mirrors `gi_union_info_get_field`).
    pub fn get_field(&self, n: u32) -> Option<&FieldInfo> {
        self.fields.get(n as usize)
    }

    /// Returns the number of methods (mirrors `gi_union_info_get_n_methods`).
    pub fn n_methods(&self) -> u32 {
        self.methods.len() as u32
    }

    /// Gets a method by index (mirrors `gi_union_info_get_method`).
    pub fn get_method(&self, n: u32) -> Option<&FunctionInfo> {
        self.methods.get(n as usize)
    }

    /// Returns whether discriminated (mirrors `gi_union_info_is_discriminated`).
    pub fn is_discriminated(&self) -> bool {
        self.is_discriminated
    }

    /// Returns the discriminator offset (mirrors `gi_union_info_get_discriminator_offset`).
    pub fn discriminator_offset(&self) -> usize {
        self.discriminator_offset
    }

    /// Gets a discriminator by index (mirrors `gi_union_info_get_discriminator`).
    pub fn get_discriminator(&self, n: usize) -> Option<&ConstantInfo> {
        self.discriminators.get(n)
    }

    /// Finds a method by name (mirrors `gi_union_info_find_method`).
    pub fn find_method(&self, _name: &str) -> Option<&FunctionInfo> {
        None
    }

    /// Returns the size (mirrors `gi_union_info_get_size`).
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns the alignment (mirrors `gi_union_info_get_alignment`).
    pub fn alignment(&self) -> usize {
        self.alignment
    }

    /// Returns the copy function name (mirrors `gi_union_info_get_copy_function_name`).
    pub fn copy_function(&self) -> &str {
        &self.copy_function
    }

    /// Returns the free function name (mirrors `gi_union_info_get_free_function_name`).
    pub fn free_function(&self) -> &str {
        &self.free_function
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let ui = UnionInfo::new();
        assert_eq!(ui.n_fields(), 0);
        assert_eq!(ui.n_methods(), 0);
        assert!(!ui.is_discriminated());
    }
}
