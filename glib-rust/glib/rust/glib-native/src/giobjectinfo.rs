//! `giobjectinfo` matching `girepository/giobjectinfo.h`.
//!
//! Object info: describes a GObject class type.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::giconstantinfo::ConstantInfo;
use crate::gifieldinfo::FieldInfo;
use crate::gifunctioninfo::FunctionInfo;
use crate::giinterfaceinfo::InterfaceInfo;
use crate::gipropertyinfo::PropertyInfo;
use crate::gisignalinfo::SignalInfo;
use crate::gistructinfo::StructInfo;
use crate::givfuncinfo::VFuncInfo;
use alloc::string::String;
use alloc::vec::Vec;

/// Object info (mirrors `GIObjectInfo`).
#[derive(Debug, Clone, Default)]
pub struct ObjectInfo {
    pub type_name: String,
    pub type_init: String,
    pub abstract_: bool,
    pub final_: bool,
    pub fundamental: bool,
    pub parent: Option<String>,
    pub interfaces: Vec<InterfaceInfo>,
    pub fields: Vec<FieldInfo>,
    pub properties: Vec<PropertyInfo>,
    pub methods: Vec<FunctionInfo>,
    pub signals: Vec<SignalInfo>,
    pub vfuncs: Vec<VFuncInfo>,
    pub constants: Vec<ConstantInfo>,
    pub class_struct: Option<StructInfo>,
    pub ref_function: String,
    pub unref_function: String,
    pub set_value_function: String,
    pub get_value_function: String,
}

impl ObjectInfo {
    /// Creates a new object info.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the type name (mirrors `gi_object_info_get_type_name`).
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// Returns the type init function name
    /// (mirrors `gi_object_info_get_type_init_function_name`).
    pub fn type_init(&self) -> &str {
        &self.type_init
    }

    /// Returns whether abstract (mirrors `gi_object_info_get_abstract`).
    pub fn is_abstract(&self) -> bool {
        self.abstract_
    }

    /// Returns whether final (mirrors `gi_object_info_get_final`).
    pub fn is_final(&self) -> bool {
        self.final_
    }

    /// Returns whether fundamental (mirrors `gi_object_info_get_fundamental`).
    pub fn is_fundamental(&self) -> bool {
        self.fundamental
    }

    /// Returns the number of interfaces (mirrors `gi_object_info_get_n_interfaces`).
    pub fn n_interfaces(&self) -> u32 {
        self.interfaces.len() as u32
    }

    /// Gets an interface by index (mirrors `gi_object_info_get_interface`).
    pub fn get_interface(&self, n: u32) -> Option<&InterfaceInfo> {
        self.interfaces.get(n as usize)
    }

    /// Returns the number of fields (mirrors `gi_object_info_get_n_fields`).
    pub fn n_fields(&self) -> u32 {
        self.fields.len() as u32
    }

    /// Gets a field by index (mirrors `gi_object_info_get_field`).
    pub fn get_field(&self, n: u32) -> Option<&FieldInfo> {
        self.fields.get(n as usize)
    }

    /// Returns the number of properties (mirrors `gi_object_info_get_n_properties`).
    pub fn n_properties(&self) -> u32 {
        self.properties.len() as u32
    }

    /// Gets a property by index (mirrors `gi_object_info_get_property`).
    pub fn get_property(&self, n: u32) -> Option<&PropertyInfo> {
        self.properties.get(n as usize)
    }

    /// Returns the number of methods (mirrors `gi_object_info_get_n_methods`).
    pub fn n_methods(&self) -> u32 {
        self.methods.len() as u32
    }

    /// Gets a method by index (mirrors `gi_object_info_get_method`).
    pub fn get_method(&self, n: u32) -> Option<&FunctionInfo> {
        self.methods.get(n as usize)
    }

    /// Returns the number of signals (mirrors `gi_object_info_get_n_signals`).
    pub fn n_signals(&self) -> u32 {
        self.signals.len() as u32
    }

    /// Gets a signal by index (mirrors `gi_object_info_get_signal`).
    pub fn get_signal(&self, n: u32) -> Option<&SignalInfo> {
        self.signals.get(n as usize)
    }

    /// Returns the number of vfuncs (mirrors `gi_object_info_get_n_vfuncs`).
    pub fn n_vfuncs(&self) -> u32 {
        self.vfuncs.len() as u32
    }

    /// Gets a vfunc by index (mirrors `gi_object_info_get_vfunc`).
    pub fn get_vfunc(&self, n: u32) -> Option<&VFuncInfo> {
        self.vfuncs.get(n as usize)
    }

    /// Returns the number of constants (mirrors `gi_object_info_get_n_constants`).
    pub fn n_constants(&self) -> u32 {
        self.constants.len() as u32
    }

    /// Gets a constant by index (mirrors `gi_object_info_get_constant`).
    pub fn get_constant(&self, n: u32) -> Option<&ConstantInfo> {
        self.constants.get(n as usize)
    }

    /// Returns the class struct (mirrors `gi_object_info_get_class_struct`).
    pub fn class_struct(&self) -> Option<&StructInfo> {
        self.class_struct.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let oi = ObjectInfo::new();
        assert_eq!(oi.type_name(), "");
        assert!(!oi.is_abstract());
        assert_eq!(oi.n_interfaces(), 0);
        assert_eq!(oi.n_methods(), 0);
    }

    #[test]
    fn test_custom() {
        let mut oi = ObjectInfo::new();
        oi.type_name = "GApplication".into();
        oi.abstract_ = true;
        assert_eq!(oi.type_name(), "GApplication");
        assert!(oi.is_abstract());
    }
}
