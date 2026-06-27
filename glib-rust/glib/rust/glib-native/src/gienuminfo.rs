//! GObject introspection enum info matching `girepository/gienuminfo.h`.

use crate::gibaseinfo::{BaseInfo, InfoType};
use crate::gitypeinfo::TypeTag;
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;

/// A single enum value (`GIValueInfo`).
#[derive(Clone, Debug)]
pub struct ValueInfo {
    name: String,
    value: i64,
    nick: String,
}

impl ValueInfo {
    /// Create a new enum value info.
    pub fn new(name: impl Into<String>, value: i64, nick: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value,
            nick: nick.into(),
        }
    }

    /// Returns the value name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the numeric value.
    pub fn value(&self) -> i64 {
        self.value
    }

    /// Returns the value nick.
    pub fn nick(&self) -> &str {
        &self.nick
    }
}

/// Enum introspection info (`GIEnumInfo`).
#[derive(Debug)]
pub struct EnumInfo {
    base: Arc<BaseInfo>,
    values: Vec<ValueInfo>,
    methods: Vec<Arc<BaseInfo>>,
    storage_type: TypeTag,
}

impl EnumInfo {
    /// Create a new enum info with the given values.
    pub fn new(
        name: impl Into<String>,
        namespace: impl Into<String>,
        values: &[(&str, i64, &str)],
    ) -> Arc<Self> {
        let base = BaseInfo::new(name, namespace, InfoType::Enum, None, Weak::new());
        Arc::new(Self {
            base,
            values: values
                .iter()
                .map(|(n, v, nick)| ValueInfo::new(*n, *v, *nick))
                .collect(),
            methods: Vec::new(),
            storage_type: TypeTag::Int32,
        })
    }

    /// Returns the embedded base info.
    pub fn base(&self) -> &Arc<BaseInfo> {
        &self.base
    }

    /// Returns the number of enum values (`gi_enum_info_get_n_values`).
    pub fn n_values(&self) -> usize {
        self.values.len()
    }

    /// Returns the `n`th enum value (`gi_enum_info_get_value`).
    pub fn value(&self, n: usize) -> Option<&ValueInfo> {
        self.values.get(n)
    }

    /// Returns the number of methods (`gi_enum_info_get_n_methods`).
    pub fn n_methods(&self) -> usize {
        self.methods.len()
    }

    /// Returns the `n`th method (`gi_enum_info_get_method`).
    pub fn method(&self, n: usize) -> Option<Arc<BaseInfo>> {
        self.methods.get(n).map(Arc::clone)
    }

    /// Returns the storage type (`gi_enum_info_get_storage_type`).
    pub fn storage_type(&self) -> TypeTag {
        self.storage_type
    }

    /// Bump the ref count via the embedded base info.
    pub fn ref_(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_info_values_and_methods() {
        let info = EnumInfo::new("MyEnum", "Test", &[("ZERO", 0, "zero"), ("ONE", 1, "one")]);
        assert_eq!(info.n_values(), 2);
        assert_eq!(info.n_methods(), 0);
        let v = info.value(1).expect("second value");
        assert_eq!(v.name(), "ONE");
        assert_eq!(v.value(), 1);
        assert_eq!(info.base().info_type(), InfoType::Enum);
    }
}
