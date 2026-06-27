//! `gitypes` matching `girepository/gitypes.h`.
//!
//! Core type definitions for GObject introspection: enums, flags,
//! and the `GIArgument` union.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use alloc::string::String;

/// Transfer ownership mode (mirrors `GITransfer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum GITransfer {
    #[default]
    Nothing = 0,
    Container = 1,
    Everything = 2,
}

/// Argument direction (mirrors `GIDirection`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum GIDirection {
    #[default]
    In = 0,
    Out = 1,
    Inout = 2,
}

/// Callback scope type (mirrors `GIScopeType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum GIScopeType {
    #[default]
    Invalid = 0,
    Call = 1,
    Async = 2,
    Notified = 3,
    Forever = 4,
}

/// Type tag for a `GITypeInfo` (mirrors `GITypeTag`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum GITypeTag {
    #[default]
    Void = 0,
    Boolean = 1,
    Int8 = 2,
    Uint8 = 3,
    Int16 = 4,
    Uint16 = 5,
    Int32 = 6,
    Uint32 = 7,
    Int64 = 8,
    Uint64 = 9,
    Float = 10,
    Double = 11,
    Gtype = 12,
    Utf8 = 13,
    Filename = 14,
    Array = 15,
    Interface = 16,
    Glist = 17,
    Gslist = 18,
    Ghash = 19,
    Error = 20,
    Unichar = 21,
}

/// Number of type tags (mirrors `GI_TYPE_TAG_N_TYPES`).
pub const GI_TYPE_TAG_N_TYPES: usize = 22;

/// Checks if a type tag is basic (mirrors `GI_TYPE_TAG_IS_BASIC`).
pub fn type_tag_is_basic(tag: GITypeTag) -> bool {
    (tag as u32) < (GITypeTag::Array as u32) || tag == GITypeTag::Unichar
}

/// Checks if a type tag is numeric (mirrors `GI_TYPE_TAG_IS_NUMERIC`).
pub fn type_tag_is_numeric(tag: GITypeTag) -> bool {
    (tag as u32) >= (GITypeTag::Int8 as u32) && (tag as u32) <= (GITypeTag::Double as u32)
}

/// Checks if a type tag is a container (mirrors `GI_TYPE_TAG_IS_CONTAINER`).
pub fn type_tag_is_container(tag: GITypeTag) -> bool {
    tag == GITypeTag::Array
        || (tag as u32) >= (GITypeTag::Glist as u32) && (tag as u32) <= (GITypeTag::Ghash as u32)
}

/// Array type (mirrors `GIArrayType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum GIArrayType {
    #[default]
    C = 0,
    Array = 1,
    PtrArray = 2,
    ByteArray = 3,
}

/// Field info flags (mirrors `GIFieldInfoFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct GIFieldInfoFlags(pub u32);

impl GIFieldInfoFlags {
    pub const NONE: Self = Self(0);
    pub const READABLE: Self = Self(1 << 0);
    pub const WRITABLE: Self = Self(1 << 1);
}

/// VFunc info flags (mirrors `GIVFuncInfoFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct GIVFuncInfoFlags(pub u32);

impl GIVFuncInfoFlags {
    pub const NONE: Self = Self(0);
    pub const MUST_CHAIN_UP: Self = Self(1 << 0);
    pub const MUST_OVERRIDE: Self = Self(1 << 1);
    pub const MUST_NOT_OVERRIDE: Self = Self(1 << 2);
}

/// Function info flags (mirrors `GIFunctionInfoFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct GIFunctionInfoFlags(pub u32);

impl GIFunctionInfoFlags {
    pub const NONE: Self = Self(0);
    pub const IS_METHOD: Self = Self(1 << 0);
    pub const IS_CONSTRUCTOR: Self = Self(1 << 1);
    pub const IS_GETTER: Self = Self(1 << 2);
    pub const IS_SETTER: Self = Self(1 << 3);
    pub const WRAPS_VFUNC: Self = Self(1 << 4);
    pub const IS_ASYNC: Self = Self(1 << 5);
}

/// Polymorphic argument value (mirrors `GIArgument` union).
#[derive(Clone, Debug, Default)]
pub struct GIArgument {
    pub v_boolean: bool,
    pub v_int8: i8,
    pub v_uint8: u8,
    pub v_int16: i16,
    pub v_uint16: u16,
    pub v_int32: i32,
    pub v_uint32: u32,
    pub v_int64: i64,
    pub v_uint64: u64,
    pub v_float: f32,
    pub v_double: f64,
    pub v_short: i16,
    pub v_ushort: u16,
    pub v_int: i32,
    pub v_uint: u32,
    pub v_long: i64,
    pub v_ulong: u64,
    pub v_ssize: isize,
    pub v_size: usize,
    pub v_string: Option<String>,
}

impl GIArgument {
    pub fn new_string(s: &str) -> Self {
        Self {
            v_string: Some(s.into()),
            ..Default::default()
        }
    }

    pub fn new_int32(val: i32) -> Self {
        Self {
            v_int32: val,
            ..Default::default()
        }
    }

    pub fn new_boolean(val: bool) -> Self {
        Self {
            v_boolean: val,
            ..Default::default()
        }
    }
}

/// Info type kind (mirrors the `GType` hierarchy of `GIBaseInfo` subtypes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum GIInfoType {
    #[default]
    Invalid = 0,
    Function = 1,
    Callback = 2,
    Struct = 3,
    Boxed = 4,
    Enum = 5,
    Flags = 6,
    Object = 7,
    Interface = 8,
    Constant = 9,
    Invalid_ = 10,
    Union = 11,
    Value = 12,
    Signal = 13,
    Vfunc = 14,
    Property = 15,
    Field = 16,
    Arg = 17,
    Type = 18,
    Unresolved = 19,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_tag_is_basic() {
        assert!(type_tag_is_basic(GITypeTag::Void));
        assert!(type_tag_is_basic(GITypeTag::Boolean));
        assert!(type_tag_is_basic(GITypeTag::Unichar));
        assert!(!type_tag_is_basic(GITypeTag::Array));
        assert!(!type_tag_is_basic(GITypeTag::Glist));
    }

    #[test]
    fn test_type_tag_is_numeric() {
        assert!(type_tag_is_numeric(GITypeTag::Int8));
        assert!(type_tag_is_numeric(GITypeTag::Double));
        assert!(!type_tag_is_numeric(GITypeTag::Void));
        assert!(!type_tag_is_numeric(GITypeTag::Utf8));
    }

    #[test]
    fn test_type_tag_is_container() {
        assert!(type_tag_is_container(GITypeTag::Array));
        assert!(type_tag_is_container(GITypeTag::Glist));
        assert!(type_tag_is_container(GITypeTag::Ghash));
        assert!(!type_tag_is_container(GITypeTag::Void));
        assert!(!type_tag_is_container(GITypeTag::Interface));
    }

    #[test]
    fn test_gi_argument() {
        let arg = GIArgument::new_string("hello");
        assert_eq!(arg.v_string, Some("hello".to_string()));

        let arg = GIArgument::new_int32(42);
        assert_eq!(arg.v_int32, 42);

        let arg = GIArgument::new_boolean(true);
        assert!(arg.v_boolean);
    }

    #[test]
    fn test_transfer() {
        assert_eq!(GITransfer::Nothing, GITransfer::default());
        assert_ne!(GITransfer::Everything, GITransfer::Nothing);
    }

    #[test]
    fn test_function_flags() {
        assert!(GIFunctionInfoFlags::IS_METHOD.0 & 1 != 0);
        assert!(GIFunctionInfoFlags::IS_CONSTRUCTOR.0 & 2 != 0);
        assert_eq!(GIFunctionInfoFlags::NONE.0, 0);
    }
}
