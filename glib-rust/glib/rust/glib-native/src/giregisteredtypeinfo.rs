//! `giregisteredtypeinfo` matching `girepository/giregisteredtypeinfo.h`.
//!
//! Registered type info: base for types registered with the GObject type system.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::prelude::*;
use alloc::string::String;

/// Registered type info (mirrors `GIRegisteredTypeInfo`).
#[derive(Debug, Clone, Default)]
pub struct RegisteredTypeInfo {
    pub type_name: String,
    pub type_init: String,
    pub g_type: u64,
    pub is_boxed: bool,
}

impl RegisteredTypeInfo {
    /// Creates a new registered type info.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the type name (mirrors `gi_registered_type_info_get_type_name`).
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// Returns the type init function name
    /// (mirrors `gi_registered_type_info_get_type_init_function_name`).
    pub fn type_init(&self) -> &str {
        &self.type_init
    }

    /// Returns the GType (mirrors `gi_registered_type_info_get_g_type`).
    pub fn g_type(&self) -> u64 {
        self.g_type
    }

    /// Returns whether boxed (mirrors `gi_registered_type_info_is_boxed`).
    pub fn is_boxed(&self) -> bool {
        self.is_boxed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let rti = RegisteredTypeInfo::new();
        assert_eq!(rti.type_name(), "");
        assert_eq!(rti.g_type(), 0);
        assert!(!rti.is_boxed());
    }

    #[test]
    fn test_custom() {
        let mut rti = RegisteredTypeInfo::new();
        rti.type_name = "GObject".into();
        rti.g_type = 20;
        assert_eq!(rti.type_name(), "GObject");
        assert_eq!(rti.g_type(), 20);
    }
}
