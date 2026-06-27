//! GObject introspection type info matching `girepository/gitypeinfo.h`.
//!
//! Describes the type of a value, argument, field, etc.

use crate::gibaseinfo::{BaseInfo, InfoType};
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;

/// The type tag of a [class@GIRepository.TypeInfo] (`GITypeTag`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TypeTag {
    /// `void`
    Void = 0,
    /// `gboolean`
    Boolean = 1,
    /// 8-bit signed integer.
    Int8 = 2,
    /// 8-bit unsigned integer.
    UInt8 = 3,
    /// 16-bit signed integer.
    Int16 = 4,
    /// 16-bit unsigned integer.
    UInt16 = 5,
    /// 32-bit signed integer.
    Int32 = 6,
    /// 32-bit unsigned integer.
    UInt32 = 7,
    /// 64-bit signed integer.
    Int64 = 8,
    /// 64-bit unsigned integer.
    UInt64 = 9,
    /// `float`
    Float = 10,
    /// `double`
    Double = 11,
    /// `GType`
    GType = 12,
    /// UTF-8 encoded string.
    Utf8 = 13,
    /// Filename encoded in the native filesystem encoding.
    Filename = 14,
    /// C array.
    Array = 15,
    /// Extended interface object.
    Interface = 16,
    /// `GList`
    GList = 17,
    /// `GSList`
    GSList = 18,
    /// `GHashTable`
    GHash = 19,
    /// `GError`
    Error = 20,
    /// Unicode character.
    Unichar = 21,
}

impl TypeTag {
    /// Number of defined type tags (`GI_TYPE_TAG_N_TYPES`).
    pub const N_TYPES: usize = Self::Unichar as usize + 1;

    /// Returns `true` if `tag` is a basic type (`GI_TYPE_TAG_IS_BASIC`).
    pub fn is_basic(self) -> bool {
        (self as u8) < Self::Array as u8 || self == Self::Unichar
    }

    /// Returns `true` if `tag` is numeric (`GI_TYPE_TAG_IS_NUMERIC`).
    pub fn is_numeric(self) -> bool {
        (self as u8) >= (Self::Int8 as u8) && (self as u8) <= (Self::Double as u8)
    }

    /// Returns `true` if `tag` is a container (`GI_TYPE_TAG_IS_CONTAINER`).
    pub fn is_container(self) -> bool {
        matches!(self, Self::Array | Self::GList | Self::GSList | Self::GHash)
    }

    /// Returns a debug string (`gi_type_tag_to_string`).
    pub fn to_string(self) -> &'static str {
        match self {
            Self::Void => "void",
            Self::Boolean => "gboolean",
            Self::Int8 => "gint8",
            Self::UInt8 => "guint8",
            Self::Int16 => "gint16",
            Self::UInt16 => "guint16",
            Self::Int32 => "gint32",
            Self::UInt32 => "guint32",
            Self::Int64 => "gint64",
            Self::UInt64 => "guint64",
            Self::Float => "gfloat",
            Self::Double => "gdouble",
            Self::GType => "GType",
            Self::Utf8 => "utf8",
            Self::Filename => "filename",
            Self::Array => "array",
            Self::Interface => "interface",
            Self::GList => "GList",
            Self::GSList => "GSList",
            Self::GHash => "GHashTable",
            Self::Error => "GError",
            Self::Unichar => "gunichar",
        }
    }
}

/// Type information (`GITypeInfo`).
#[derive(Debug)]
pub struct TypeInfo {
    base: Arc<BaseInfo>,
    tag: TypeTag,
    is_pointer: bool,
    param_types: Vec<Arc<TypeInfo>>,
}

impl TypeInfo {
    /// Create a simple type info node.
    pub fn new(
        name: impl Into<String>,
        namespace: impl Into<String>,
        tag: TypeTag,
        is_pointer: bool,
    ) -> Arc<Self> {
        let base = BaseInfo::new(name, namespace, InfoType::Type, None, Weak::new());
        Arc::new(Self {
            base,
            tag,
            is_pointer,
            param_types: Vec::new(),
        })
    }

    /// Create a container type with parameter types.
    pub fn new_container(
        name: impl Into<String>,
        namespace: impl Into<String>,
        tag: TypeTag,
        is_pointer: bool,
        param_types: Vec<Arc<TypeInfo>>,
    ) -> Arc<Self> {
        let base = BaseInfo::new(name, namespace, InfoType::Type, None, Weak::new());
        Arc::new(Self {
            base,
            tag,
            is_pointer,
            param_types,
        })
    }

    /// Returns the embedded base info.
    pub fn base(&self) -> &Arc<BaseInfo> {
        &self.base
    }

    /// Returns `true` if the type is a pointer (`gi_type_info_is_pointer`).
    pub fn is_pointer(&self) -> bool {
        self.is_pointer
    }

    /// Returns the type tag (`gi_type_info_get_tag`).
    pub fn tag(&self) -> TypeTag {
        self.tag
    }

    /// Returns the `n`th parameter type (`gi_type_info_get_param_type`).
    pub fn param_type(&self, n: usize) -> Option<Arc<TypeInfo>> {
        self.param_types.get(n).map(Arc::clone)
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
    fn type_tag_numeric_and_basic_helpers() {
        assert!(TypeTag::Int32.is_numeric());
        assert!(!TypeTag::Utf8.is_numeric());
        assert!(TypeTag::Boolean.is_basic());
        assert!(TypeTag::Array.is_container());
    }

    #[test]
    fn type_info_tag_and_pointer() {
        let info = TypeInfo::new("str_ptr", "Test", TypeTag::Utf8, true);
        assert_eq!(info.tag(), TypeTag::Utf8);
        assert!(info.is_pointer());
        assert_eq!(info.base().info_type(), InfoType::Type);
    }

    #[test]
    fn type_info_param_type_lookup() {
        let elem = TypeInfo::new("elem", "Test", TypeTag::Int32, false);
        let array = TypeInfo::new_container("arr", "Test", TypeTag::Array, true, vec![elem.ref_()]);
        let param = array.param_type(0).expect("element type");
        assert_eq!(param.tag(), TypeTag::Int32);
    }
}
