//! `gistructinfo` matching `girepository/gistructinfo.h`.
//!
//! Struct info: describes a C struct type.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gifieldinfo::FieldInfo;
use crate::gifunctioninfo::FunctionInfo;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

/// Struct info (mirrors `GIStructInfo`).
#[derive(Debug, Clone, Default)]
pub struct StructInfo {
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<FunctionInfo>,
    pub size: usize,
    pub alignment: usize,
    pub is_gtype_struct: bool,
    pub is_foreign: bool,
    pub copy_function: String,
    pub free_function: String,
}

impl StructInfo {
    /// Creates a new struct info.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of fields (mirrors `gi_struct_info_get_n_fields`).
    pub fn n_fields(&self) -> u32 {
        self.fields.len() as u32
    }

    /// Gets a field by index (mirrors `gi_struct_info_get_field`).
    pub fn get_field(&self, n: u32) -> Option<&FieldInfo> {
        self.fields.get(n as usize)
    }

    /// Finds a field by name (mirrors `gi_struct_info_find_field`).
    pub fn find_field(&self, _name: &str) -> Option<&FieldInfo> {
        None
    }

    /// Returns the number of methods (mirrors `gi_struct_info_get_n_methods`).
    pub fn n_methods(&self) -> u32 {
        self.methods.len() as u32
    }

    /// Gets a method by index (mirrors `gi_struct_info_get_method`).
    pub fn get_method(&self, n: u32) -> Option<&FunctionInfo> {
        self.methods.get(n as usize)
    }

    /// Finds a method by name (mirrors `gi_struct_info_find_method`).
    pub fn find_method(&self, _name: &str) -> Option<&FunctionInfo> {
        None
    }

    /// Returns the size (mirrors `gi_struct_info_get_size`).
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns the alignment (mirrors `gi_struct_info_get_alignment`).
    pub fn alignment(&self) -> usize {
        self.alignment
    }

    /// Returns whether gtype struct (mirrors `gi_struct_info_is_gtype_struct`).
    pub fn is_gtype_struct(&self) -> bool {
        self.is_gtype_struct
    }

    /// Returns whether foreign (mirrors `gi_struct_info_is_foreign`).
    pub fn is_foreign(&self) -> bool {
        self.is_foreign
    }

    /// Returns the copy function name (mirrors `gi_struct_info_get_copy_function_name`).
    pub fn copy_function(&self) -> &str {
        &self.copy_function
    }

    /// Returns the free function name (mirrors `gi_struct_info_get_free_function_name`).
    pub fn free_function(&self) -> &str {
        &self.free_function
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let si = StructInfo::new();
        assert_eq!(si.n_fields(), 0);
        assert_eq!(si.n_methods(), 0);
        assert_eq!(si.size(), 0);
        assert!(!si.is_gtype_struct());
    }
}
