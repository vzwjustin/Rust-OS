//! GValue — polymorphic value container (`GValue` in C).
//!
//! A `GValue` can hold a value of any registered `GType`. It is initialized
//! with a type, then set/cast to that type's value.

use crate::gtype::*;
use crate::prelude::*;
use alloc::sync::Arc;

/// A polymorphic value container (`GValue`).
#[derive(Clone, Default)]
pub struct GValue {
    pub g_type: GType,
    pub data: GValueData,
}

impl GValue {
    /// Create a new uninitialized GValue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize a GValue for `type_id` (`g_value_init`).
    pub fn init(&mut self, type_id: GType) {
        self.g_type = type_id;
        self.data = GValueData::default();
        if let Some(vt) = type_value_table(type_id) {
            (vt.value_init)(&mut self.data);
        }
    }

    /// Create an initialized GValue for `type_id`.
    pub fn for_type(type_id: GType) -> Self {
        let mut v = Self::new();
        v.init(type_id);
        v
    }

    /// Reset the GValue to its default state (`g_value_reset`).
    pub fn reset(&mut self) {
        if self.g_type != G_TYPE_INVALID {
            if let Some(vt) = type_value_table(self.g_type) {
                (vt.value_free)(&mut self.data);
            }
        }
        self.data = GValueData::default();
    }

    /// Clear the GValue, freeing contents (`g_value_clear`).
    pub fn clear(&mut self) {
        if self.g_type != G_TYPE_INVALID {
            if let Some(vt) = type_value_table(self.g_type) {
                (vt.value_free)(&mut self.data);
            }
        }
        self.g_type = G_TYPE_INVALID;
        self.data = GValueData::default();
    }

    /// Get the type of this value (`G_VALUE_TYPE`).
    pub fn value_type(&self) -> GType {
        self.g_type
    }

    /// Check if the value holds type `type_id` (`G_VALUE_HOLDS`).
    pub fn holds(&self, type_id: GType) -> bool {
        self.g_type == type_id
    }

    /// Copy from another GValue (`g_value_copy`).
    pub fn copy_from(&mut self, src: &GValue) {
        if src.g_type == G_TYPE_INVALID {
            return;
        }
        if self.g_type != src.g_type {
            self.clear();
            self.init(src.g_type);
        }
        if let Some(vt) = type_value_table(src.g_type) {
            (vt.value_copy)(&src.data, &mut self.data);
        } else {
            self.data = src.data.clone();
        }
    }

    // ── Boolean ───────────────────────────────────────────────────

    pub fn set_boolean(&mut self, v: bool) {
        self.data.v_uint = if v { 1 } else { 0 };
    }
    pub fn get_boolean(&self) -> bool {
        self.data.v_uint != 0
    }

    // ── Int ───────────────────────────────────────────────────────

    pub fn set_int(&mut self, v: i32) {
        self.data.v_int = v;
    }
    pub fn get_int(&self) -> i32 {
        self.data.v_int
    }

    pub fn set_uint(&mut self, v: u32) {
        self.data.v_uint = v;
    }
    pub fn get_uint(&self) -> u32 {
        self.data.v_uint
    }

    // ── Int64 / UInt64 ────────────────────────────────────────────

    pub fn set_int64(&mut self, v: i64) {
        self.data.v_long = v;
    }
    pub fn get_int64(&self) -> i64 {
        self.data.v_long
    }

    pub fn set_uint64(&mut self, v: u64) {
        self.data.v_ulong = v;
    }
    pub fn get_uint64(&self) -> u64 {
        self.data.v_ulong
    }

    // ── Float / Double ────────────────────────────────────────────

    pub fn set_float(&mut self, v: f32) {
        self.data.v_float = v;
    }
    pub fn get_float(&self) -> f32 {
        self.data.v_float
    }

    pub fn set_double(&mut self, v: f64) {
        self.data.v_double = v;
    }
    pub fn get_double(&self) -> f64 {
        self.data.v_double
    }

    // ── Char / UChar ──────────────────────────────────────────────

    pub fn set_char(&mut self, v: i8) {
        self.data.v_int = v as i32;
    }
    pub fn get_char(&self) -> i8 {
        self.data.v_int as i8
    }

    pub fn set_uchar(&mut self, v: u8) {
        self.data.v_uint = v as u32;
    }
    pub fn get_uchar(&self) -> u8 {
        self.data.v_uint as u8
    }

    // ── Long / ULong ──────────────────────────────────────────────

    pub fn set_long(&mut self, v: i64) {
        self.data.v_long = v;
    }
    pub fn get_long(&self) -> i64 {
        self.data.v_long
    }

    pub fn set_ulong(&mut self, v: u64) {
        self.data.v_ulong = v;
    }
    pub fn get_ulong(&self) -> u64 {
        self.data.v_ulong
    }

    // ── String ────────────────────────────────────────────────────

    pub fn set_string(&mut self, v: &str) {
        self.data.v_pointer = Some(Arc::new(String::from(v)));
    }
    pub fn get_string(&self) -> Option<&str> {
        self.data.v_pointer.as_ref().and_then(|p| {
            p.downcast_ref::<String>().map(|s| s.as_str())
        })
    }

    // ── Pointer ───────────────────────────────────────────────────

    pub fn set_pointer(&mut self, v: Arc<core::any::Any>) {
        self.data.v_pointer = Some(v);
    }
    pub fn get_pointer(&self) -> Option<&Arc<core::any::Any>> {
        self.data.v_pointer.as_ref()
    }

    // ── Enum / Flags ──────────────────────────────────────────────

    pub fn set_enum(&mut self, v: i32) {
        self.data.v_int = v;
    }
    pub fn get_enum(&self) -> i32 {
        self.data.v_int
    }

    pub fn set_flags(&mut self, v: u32) {
        self.data.v_uint = v;
    }
    pub fn get_flags(&self) -> u32 {
        self.data.v_uint
    }

    // ── Object ────────────────────────────────────────────────────

    pub fn set_object(&mut self, v: Arc<core::any::Any>) {
        self.data.v_pointer = Some(v);
    }
    pub fn get_object<T: 'static>(&self) -> Option<Arc<T>> {
        self.data.v_pointer.as_ref().and_then(|p| {
            p.clone().downcast::<T>().ok()
        })
    }

    // ── Boxed ─────────────────────────────────────────────────────

    pub fn set_boxed(&mut self, v: Arc<core::any::Any>) {
        self.data.v_pointer = Some(v);
    }
    pub fn get_boxed<T: 'static>(&self) -> Option<Arc<T>> {
        self.data.v_pointer.as_ref().and_then(|p| {
            p.clone().downcast::<T>().ok()
        })
    }
}

/// Create a GValue holding a boolean.
pub fn value_new_boolean(v: bool) -> GValue {
    let mut val = GValue::for_type(G_TYPE_BOOLEAN);
    val.set_boolean(v);
    val
}

/// Create a GValue holding an int.
pub fn value_new_int(v: i32) -> GValue {
    let mut val = GValue::for_type(G_TYPE_INT);
    val.set_int(v);
    val
}

/// Create a GValue holding a string.
pub fn value_new_string(v: &str) -> GValue {
    let mut val = GValue::for_type(G_TYPE_STRING);
    val.set_string(v);
    val
}

/// Create a GValue holding a double.
pub fn value_new_double(v: f64) -> GValue {
    let mut val = GValue::for_type(G_TYPE_DOUBLE);
    val.set_double(v);
    val
}

/// Create a GValue holding a uint.
pub fn value_new_uint(v: u32) -> GValue {
    let mut val = GValue::for_type(G_TYPE_UINT);
    val.set_uint(v);
    val
}

/// Create a GValue holding an int64.
pub fn value_new_int64(v: i64) -> GValue {
    let mut val = GValue::for_type(G_TYPE_INT64);
    val.set_int64(v);
    val
}

/// Create a GValue holding a uint64.
pub fn value_new_uint64(v: u64) -> GValue {
    let mut val = GValue::for_type(G_TYPE_UINT64);
    val.set_uint64(v);
    val
}

/// Create a GValue holding a float.
pub fn value_new_float(v: f32) -> GValue {
    let mut val = GValue::for_type(G_TYPE_FLOAT);
    val.set_float(v);
    val
}

/// Create a GValue holding a char.
pub fn value_new_char(v: i8) -> GValue {
    let mut val = GValue::for_type(G_TYPE_CHAR);
    val.set_char(v);
    val
}

/// Create a GValue holding an enum value.
pub fn value_new_enum(v: i32) -> GValue {
    let mut val = GValue::for_type(G_TYPE_ENUM);
    val.set_enum(v);
    val
}

/// Create a GValue holding flags.
pub fn value_new_flags(v: u32) -> GValue {
    let mut val = GValue::for_type(G_TYPE_FLAGS);
    val.set_flags(v);
    val
}

/// Create a GValue holding a pointer.
pub fn value_new_pointer(v: Arc<core::any::Any>) -> GValue {
    let mut val = GValue::for_type(G_TYPE_POINTER);
    val.set_pointer(v);
    val
}

/// Create a GValue holding an object.
pub fn value_new_object(v: Arc<core::any::Any>) -> GValue {
    let mut val = GValue::for_type(G_TYPE_OBJECT);
    val.set_object(v);
    val
}

/// Create a GValue holding a boxed type.
pub fn value_new_boxed(v: Arc<core::any::Any>) -> GValue {
    let mut val = GValue::for_type(G_TYPE_BOXED);
    val.set_boxed(v);
    val
}

/// Value transform function signature.
pub type TransformFunc = fn(&GValue, &mut GValue);

/// Default value table for basic types.
pub fn default_value_table_for(type_id: GType) -> Option<GTypeValueTable> {
    match type_id {
        G_TYPE_BOOLEAN | G_TYPE_CHAR | G_TYPE_UCHAR |
        G_TYPE_INT | G_TYPE_UINT | G_TYPE_LONG | G_TYPE_ULONG |
        G_TYPE_INT64 | G_TYPE_UINT64 | G_TYPE_ENUM | G_TYPE_FLAGS => Some(GTypeValueTable {
            value_init: |_| {},
            value_free: |_| {},
            value_copy: |src, dst| { *dst = src.clone(); },
            collect_format: "i",
            lcopy_format: "p",
        }),
        G_TYPE_FLOAT | G_TYPE_DOUBLE => Some(GTypeValueTable {
            value_init: |_| {},
            value_free: |_| {},
            value_copy: |src, dst| { *dst = src.clone(); },
            collect_format: "d",
            lcopy_format: "p",
        }),
        G_TYPE_STRING | G_TYPE_POINTER | G_TYPE_OBJECT | G_TYPE_BOXED => Some(GTypeValueTable {
            value_init: |_| {},
            value_free: |_| {},
            value_copy: |src, dst| { *dst = src.clone(); },
            collect_format: "p",
            lcopy_format: "p",
        }),
        _ => None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boolean_value() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_BOOLEAN);
        v.set_boolean(true);
        assert!(v.get_boolean());
        assert!(v.holds(G_TYPE_BOOLEAN));
        v.set_boolean(false);
        assert!(!v.get_boolean());
    }

    #[test]
    fn int_value() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_INT);
        v.set_int(42);
        assert_eq!(v.get_int(), 42);
    }

    #[test]
    fn string_value() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_STRING);
        v.set_string("hello");
        assert_eq!(v.get_string(), Some("hello"));
    }

    #[test]
    fn double_value() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_DOUBLE);
        v.set_double(3.14);
        assert!((v.get_double() - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn copy_value() {
        type_init();
        let mut src = GValue::for_type(G_TYPE_INT);
        src.set_int(99);
        let mut dst = GValue::new();
        dst.copy_from(&src);
        assert_eq!(dst.get_int(), 99);
        assert!(dst.holds(G_TYPE_INT));
    }

    #[test]
    fn clear_value() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_STRING);
        v.set_string("test");
        v.clear();
        assert_eq!(v.value_type(), G_TYPE_INVALID);
        assert_eq!(v.get_string(), None);
    }

    #[test]
    fn value_new_helpers() {
        type_init();
        let v = value_new_int(7);
        assert_eq!(v.get_int(), 7);
        let v = value_new_boolean(true);
        assert!(v.get_boolean());
        let v = value_new_string("world");
        assert_eq!(v.get_string(), Some("world"));
        let v = value_new_double(2.71);
        assert!((v.get_double() - 2.71).abs() < f64::EPSILON);
    }

    #[test]
    fn enum_flags_values() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_ENUM);
        v.set_enum(5);
        assert_eq!(v.get_enum(), 5);
        let mut v = GValue::for_type(G_TYPE_FLAGS);
        v.set_flags(0x3);
        assert_eq!(v.get_flags(), 0x3);
    }

    #[test]
    fn int64_uint64_values() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_INT64);
        v.set_int64(i64::MAX);
        assert_eq!(v.get_int64(), i64::MAX);
        let mut v = GValue::for_type(G_TYPE_UINT64);
        v.set_uint64(u64::MAX);
        assert_eq!(v.get_uint64(), u64::MAX);
    }
}
