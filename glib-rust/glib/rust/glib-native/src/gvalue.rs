//! GValue — polymorphic value container (`GValue` in C).
//!
//! A `GValue` can hold a value of any registered `GType`. It is initialized
//! with a type, then set/cast to that type's value.

use crate::gtype::*;
use crate::prelude::*;
use alloc::collections::BTreeMap;
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
        self.data
            .v_pointer
            .as_ref()
            .and_then(|p| p.downcast_ref::<String>().map(|s| s.as_str()))
    }

    // ── Pointer ───────────────────────────────────────────────────

    pub fn set_pointer(&mut self, v: Arc<dyn core::any::Any + Send + Sync>) {
        self.data.v_pointer = Some(v);
    }
    pub fn get_pointer(&self) -> Option<&Arc<dyn core::any::Any + Send + Sync>> {
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

    pub fn set_object(&mut self, v: Arc<dyn core::any::Any + Send + Sync>) {
        self.data.v_pointer = Some(v);
    }
    pub fn get_object<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        self.data
            .v_pointer
            .as_ref()
            .and_then(|p| p.clone().downcast::<T>().ok())
    }

    // ── Boxed ─────────────────────────────────────────────────────

    pub fn set_boxed(&mut self, v: Arc<dyn core::any::Any + Send + Sync>) {
        self.data.v_pointer = Some(v);
    }
    pub fn get_boxed<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        self.data
            .v_pointer
            .as_ref()
            .and_then(|p| p.clone().downcast::<T>().ok())
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
pub fn value_new_pointer(v: Arc<dyn core::any::Any + Send + Sync>) -> GValue {
    let mut val = GValue::for_type(G_TYPE_POINTER);
    val.set_pointer(v);
    val
}

/// Create a GValue holding an object.
pub fn value_new_object(v: Arc<dyn core::any::Any + Send + Sync>) -> GValue {
    let mut val = GValue::for_type(G_TYPE_OBJECT);
    val.set_object(v);
    val
}

/// Create a GValue holding a boxed type.
pub fn value_new_boxed(v: Arc<dyn core::any::Any + Send + Sync>) -> GValue {
    let mut val = GValue::for_type(G_TYPE_BOXED);
    val.set_boxed(v);
    val
}

/// Value transform function signature (`GValueTransform` in C).
///
/// Reads from `src` and writes a converted representation into `dest`.
pub type TransformFunc = fn(&GValue, &mut GValue);

// ── Global transform registry ─────────────────────────────────────────
//
// Mirrors GLib's `static ... transform_func_LT` lookup table keyed by
// `(src_type, dest_type)`. Lazily initialised under `spin::Once` so the
// kernel never pays for it unless transforms are actually used.

static TRANSFORM_REGISTRY: spin::Once<spin::Mutex<BTreeMap<(GType, GType), TransformFunc>>> =
    spin::Once::new();

fn transform_registry() -> &'static spin::Mutex<BTreeMap<(GType, GType), TransformFunc>> {
    TRANSFORM_REGISTRY.call_once(|| spin::Mutex::new(BTreeMap::new()))
}

/// Register a transform function between two `GType`s (`g_value_register_transform_func`).
///
/// Subsequent registrations for the same `(src_type, dest_type)` pair replace
/// the previous one, matching upstream behaviour.
pub fn value_register_transform_func(src_type: GType, dest_type: GType, func: TransformFunc) {
    let mut reg = transform_registry().lock();
    reg.insert((src_type, dest_type), func);
}

/// Check whether a transform is possible between two types
/// (`g_value_type_transformable`).
///
/// Returns `true` when `src_type == dest_type` (identity) or when a transform
/// function has been registered for the pair.
pub fn value_can_transform(src_type: GType, dest_type: GType) -> bool {
    if src_type == dest_type {
        return true;
    }
    transform_registry()
        .lock()
        .contains_key(&(src_type, dest_type))
}

/// Transform `src_value` into `dest_value` (`g_value_transform`).
///
/// Looks up a registered transform for `(src_value.value_type(),
/// dest_value.value_type())`. On a hit the function is invoked and `true` is
/// returned. When source and destination types are equal the value is copied
/// directly (identity transform). Otherwise `false` is returned — this never
/// panics, mirroring upstream's `gboolean` return.
///
/// Note: GLib's `GTypeValueTable` carries a `value_vc_peek`/transform hook that
/// we defer (the native `GTypeValueTable` does not model it yet); only the
/// explicit registry plus identity copy are consulted here.
pub fn value_transform(src_value: &GValue, dest_value: &mut GValue) -> bool {
    let src_type = src_value.value_type();
    let dest_type = dest_value.value_type();

    if src_type == dest_type {
        dest_value.copy_from(src_value);
        return true;
    }

    let func = {
        let reg = transform_registry().lock();
        reg.get(&(src_type, dest_type)).copied()
    };

    match func {
        Some(f) => {
            f(src_value, dest_value);
            true
        }
        None => false,
    }
}

/// Default value table for basic types.
pub fn default_value_table_for(type_id: GType) -> Option<GTypeValueTable> {
    match type_id {
        G_TYPE_BOOLEAN | G_TYPE_CHAR | G_TYPE_UCHAR | G_TYPE_INT | G_TYPE_UINT | G_TYPE_LONG
        | G_TYPE_ULONG | G_TYPE_INT64 | G_TYPE_UINT64 | G_TYPE_ENUM | G_TYPE_FLAGS => {
            Some(GTypeValueTable {
                value_init: |_| {},
                value_free: |_| {},
                value_copy: |src, dst| {
                    *dst = src.clone();
                },
                collect_format: "i",
                lcopy_format: "p",
            })
        }
        G_TYPE_FLOAT | G_TYPE_DOUBLE => Some(GTypeValueTable {
            value_init: |_| {},
            value_free: |_| {},
            value_copy: |src, dst| {
                *dst = src.clone();
            },
            collect_format: "d",
            lcopy_format: "p",
        }),
        G_TYPE_STRING | G_TYPE_POINTER | G_TYPE_OBJECT | G_TYPE_BOXED => Some(GTypeValueTable {
            value_init: |_| {},
            value_free: |_| {},
            value_copy: |src, dst| {
                *dst = src.clone();
            },
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

    #[test]
    fn transform_register_and_apply() {
        type_init();
        // int -> double: cast numerically.
        value_register_transform_func(G_TYPE_INT, G_TYPE_DOUBLE, |src, dst| {
            dst.set_double(src.get_int() as f64);
        });
        assert!(value_can_transform(G_TYPE_INT, G_TYPE_DOUBLE));

        let src = value_new_int(7);
        let mut dst = GValue::for_type(G_TYPE_DOUBLE);
        assert!(value_transform(&src, &mut dst));
        assert!((dst.get_double() - 7.0).abs() < f64::EPSILON);
    }

    #[test]
    fn transform_identity() {
        type_init();
        let src = value_new_int(42);
        let mut dst = GValue::for_type(G_TYPE_INT);
        // No registered func, but src_type == dest_type -> identity copy.
        assert!(value_transform(&src, &mut dst));
        assert_eq!(dst.get_int(), 42);
    }

    #[test]
    fn transform_missing_returns_false() {
        type_init();
        // No int -> string transform registered.
        assert!(!value_can_transform(G_TYPE_INT, G_TYPE_STRING));
        let src = value_new_int(5);
        let mut dst = GValue::for_type(G_TYPE_STRING);
        assert!(!value_transform(&src, &mut dst));
    }

    #[test]
    fn transform_replaces_existing() {
        type_init();
        value_register_transform_func(G_TYPE_UINT, G_TYPE_DOUBLE, |_, dst| {
            dst.set_double(1.0);
        });
        value_register_transform_func(G_TYPE_UINT, G_TYPE_DOUBLE, |src, dst| {
            dst.set_double(src.get_uint() as f64 * 10.0);
        });
        let src = value_new_uint(3);
        let mut dst = GValue::for_type(G_TYPE_DOUBLE);
        assert!(value_transform(&src, &mut dst));
        assert!((dst.get_double() - 30.0).abs() < f64::EPSILON);
    }
}
