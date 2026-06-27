//! GObject introspection base info matching `girepository/gibaseinfo.h`.
//!
//! [class@GIRepository.BaseInfo] is the common supertype for all introspection
//! info records. Ref counting uses `Arc<T>`.

use crate::gitypelib::Typelib;
use alloc::string::String;
use alloc::sync::{Arc, Weak};

/// The type of a [class@GIRepository.BaseInfo] struct (`GIInfoType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum InfoType {
    /// Invalid type.
    Invalid = 0,
    /// Function (`GIFunctionInfo`).
    Function = 1,
    /// Callback (`GICallbackInfo`).
    Callback = 2,
    /// Struct (`GIStructInfo`).
    Struct = 3,
    /// Enum (`GIEnumInfo`).
    Enum = 5,
    /// Flags (`GIFlagsInfo`).
    Flags = 6,
    /// Object (`GIObjectInfo`).
    Object = 7,
    /// Interface (`GIInterfaceInfo`).
    Interface = 8,
    /// Constant (`GIConstantInfo`).
    Constant = 9,
    /// Union (`GIUnionInfo`).
    Union = 11,
    /// Enum value (`GIValueInfo`).
    Value = 12,
    /// Signal (`GISignalInfo`).
    Signal = 13,
    /// Virtual function (`GIVFuncInfo`).
    VFunc = 14,
    /// Property (`GIPropertyInfo`).
    Property = 15,
    /// Field (`GIFieldInfo`).
    Field = 16,
    /// Argument (`GIArgInfo`).
    Arg = 17,
    /// Type information (`GITypeInfo`).
    Type = 18,
    /// Unresolved type (`GIUnresolvedInfo`).
    Unresolved = 19,
    /// Callable (`GICallableInfo`).
    Callable = 20,
    /// Registered type (`GIRegisteredTypeInfo`).
    RegisteredType = 21,
}

impl InfoType {
    /// Number of defined info types (`GI_INFO_TYPE_N_TYPES`).
    pub const N_TYPES: usize = Self::RegisteredType as usize + 1;
}

/// Common base for all introspection info records (`GIBaseInfo`).
#[derive(Debug)]
pub struct BaseInfo {
    name: String,
    namespace: String,
    info_type: InfoType,
    deprecated: bool,
    container: Option<Arc<BaseInfo>>,
    typelib: Weak<Typelib>,
}

impl BaseInfo {
    /// Create a new base info node (internal; prefer typed constructors).
    pub(crate) fn new(
        name: impl Into<String>,
        namespace: impl Into<String>,
        info_type: InfoType,
        container: Option<Arc<BaseInfo>>,
        typelib: Weak<Typelib>,
    ) -> Arc<Self> {
        Arc::new(Self {
            name: name.into(),
            namespace: namespace.into(),
            info_type,
            deprecated: false,
            container,
            typelib,
        })
    }

    /// Returns the info name (`gi_base_info_get_name`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the namespace (`gi_base_info_get_namespace`).
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Returns the info type (`gi_base_info_get_info_type`).
    pub fn info_type(&self) -> InfoType {
        self.info_type
    }

    /// Alias for [`Self::info_type`] matching upstream naming in some bindings.
    pub fn get_type(&self) -> InfoType {
        self.info_type()
    }

    /// Returns whether the info is deprecated (`gi_base_info_is_deprecated`).
    pub fn is_deprecated(&self) -> bool {
        self.deprecated
    }

    /// Returns the container info, if any (`gi_base_info_get_container`).
    pub fn container(&self) -> Option<Arc<BaseInfo>> {
        self.container.as_ref().map(Arc::clone)
    }

    /// Returns the typelib this info belongs to (`gi_base_info_get_typelib`).
    pub fn typelib(&self) -> Option<Arc<Typelib>> {
        self.typelib.upgrade()
    }

    /// Bump the ref count (`gi_base_info_ref`).
    pub fn ref_(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(self)
    }

    /// Drop a ref count handle (`gi_base_info_unref`).
    ///
    /// With `Arc`, callers release ownership by dropping their handle.
    pub fn unref(info: Arc<Self>) {
        drop(info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_info_name_namespace_and_type() {
        let info = BaseInfo::new("Foo", "Bar", InfoType::Function, None, Weak::new());
        assert_eq!(info.name(), "Foo");
        assert_eq!(info.namespace(), "Bar");
        assert_eq!(info.get_type(), InfoType::Function);
    }

    #[test]
    fn base_info_ref_unref_roundtrip() {
        let info = BaseInfo::new("A", "Ns", InfoType::Constant, None, Weak::new());
        let second = info.ref_();
        assert!(Arc::ptr_eq(&info, &second));
        BaseInfo::unref(second);
        assert_eq!(info.name(), "A");
    }

    #[test]
    fn base_info_container_link() {
        let parent = BaseInfo::new("Parent", "Ns", InfoType::Object, None, Weak::new());
        let child = BaseInfo::new(
            "child_method",
            "Ns",
            InfoType::Function,
            Some(parent.ref_()),
            Weak::new(),
        );
        let container = child.container().expect("container");
        assert_eq!(container.name(), "Parent");
    }
}
