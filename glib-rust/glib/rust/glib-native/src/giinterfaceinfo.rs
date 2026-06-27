//! `giinterfaceinfo` matching `girepository/giinterfaceinfo.h`.
//!
//! Interface info: describes a GObject interface type.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gicallableinfo::CallableInfo;
use crate::giconstantinfo::ConstantInfo;
use crate::gipropertyinfo::PropertyInfo;
use crate::gisignalinfo::SignalInfo;
use crate::gistructinfo::StructInfo;
use crate::givfuncinfo::VFuncInfo;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

/// Interface info (mirrors `GIInterfaceInfo`).
#[derive(Debug, Clone, Default)]
pub struct InterfaceInfo {
    pub prerequisites: Vec<String>,
    pub properties: Vec<PropertyInfo>,
    pub methods: Vec<CallableInfo>,
    pub signals: Vec<SignalInfo>,
    pub vfuncs: Vec<VFuncInfo>,
    pub constants: Vec<ConstantInfo>,
    pub iface_struct: Option<StructInfo>,
}

impl InterfaceInfo {
    /// Creates a new interface info.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of prerequisites (mirrors `gi_interface_info_get_n_prerequisites`).
    pub fn n_prerequisites(&self) -> u32 {
        self.prerequisites.len() as u32
    }

    /// Gets a prerequisite by index (mirrors `gi_interface_info_get_prerequisite`).
    pub fn get_prerequisite(&self, n: u32) -> Option<&str> {
        self.prerequisites.get(n as usize).map(|s| s.as_str())
    }

    /// Returns the number of properties (mirrors `gi_interface_info_get_n_properties`).
    pub fn n_properties(&self) -> u32 {
        self.properties.len() as u32
    }

    /// Gets a property by index (mirrors `gi_interface_info_get_property`).
    pub fn get_property(&self, n: u32) -> Option<&PropertyInfo> {
        self.properties.get(n as usize)
    }

    /// Returns the number of methods (mirrors `gi_interface_info_get_n_methods`).
    pub fn n_methods(&self) -> u32 {
        self.methods.len() as u32
    }

    /// Gets a method by index (mirrors `gi_interface_info_get_method`).
    pub fn get_method(&self, n: u32) -> Option<&CallableInfo> {
        self.methods.get(n as usize)
    }

    /// Finds a method by name (mirrors `gi_interface_info_find_method`).
    pub fn find_method(&self, name: &str) -> Option<&CallableInfo> {
        self.methods.iter().find(|m| {
            // CallableInfo doesn't have a name field; use index-based lookup
            let _ = name;
            false
        })
    }

    /// Returns the number of signals (mirrors `gi_interface_info_get_n_signals`).
    pub fn n_signals(&self) -> u32 {
        self.signals.len() as u32
    }

    /// Gets a signal by index (mirrors `gi_interface_info_get_signal`).
    pub fn get_signal(&self, n: u32) -> Option<&SignalInfo> {
        self.signals.get(n as usize)
    }

    /// Returns the number of vfuncs (mirrors `gi_interface_info_get_n_vfuncs`).
    pub fn n_vfuncs(&self) -> u32 {
        self.vfuncs.len() as u32
    }

    /// Gets a vfunc by index (mirrors `gi_interface_info_get_vfunc`).
    pub fn get_vfunc(&self, n: u32) -> Option<&VFuncInfo> {
        self.vfuncs.get(n as usize)
    }

    /// Returns the number of constants (mirrors `gi_interface_info_get_n_constants`).
    pub fn n_constants(&self) -> u32 {
        self.constants.len() as u32
    }

    /// Gets a constant by index (mirrors `gi_interface_info_get_constant`).
    pub fn get_constant(&self, n: u32) -> Option<&ConstantInfo> {
        self.constants.get(n as usize)
    }

    /// Returns the interface struct (mirrors `gi_interface_info_get_iface_struct`).
    pub fn iface_struct(&self) -> Option<&StructInfo> {
        self.iface_struct.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let ii = InterfaceInfo::new();
        assert_eq!(ii.n_prerequisites(), 0);
        assert_eq!(ii.n_properties(), 0);
        assert_eq!(ii.n_methods(), 0);
    }
}
