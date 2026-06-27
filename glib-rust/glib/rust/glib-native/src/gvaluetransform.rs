//! GValue transform functions matching `gobject/gvaluetransform.c`.
//!
//! Implements the deferred Phase 9 item "GValue transform functions
//! between types". Provides:
//! - Global transform registry (`BTreeMap<(GType, GType), TransformFunc>`
//!   guarded by `spin::Mutex`).
//! - `value_register_transform_func` — register a transform.
//! - `value_type_transformable` / `value_type_compatible` — check
//!   whether a transform or copy is possible.
//! - `value_transform` — transform `src` into `dest` using a registered
//!   transform, or copy if the types are compatible.
//! - Built-in transforms between numeric types (char / uchar / int /
//!   uint / long / ulong / int64 / uint64 / float / double / boolean)
//!   and from numerics to strings, registered on first use via
//!   `init_builtin_transforms`.
//!
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::gtype::{
    type_is_a, G_TYPE_BOOLEAN, G_TYPE_CHAR, G_TYPE_DOUBLE, G_TYPE_FLOAT, G_TYPE_INT, G_TYPE_INT64,
    G_TYPE_LONG, G_TYPE_STRING, G_TYPE_UCHAR, G_TYPE_UINT, G_TYPE_UINT64, G_TYPE_ULONG,
};
use crate::gvalue::{GValue, TransformFunc};
use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::format;
use spin::mutex::Mutex;
use spin::once::Once;

// ──────────────────────────── registry ────────────────────────────────────

/// Global transform registry. Maps `(src_type, dest_type)` to a
/// `TransformFunc`. Mirrors the upstream `transform_array`
/// (`GBSearchArray`).
static TRANSFORM_REGISTRY: Once<Mutex<BTreeMap<(usize, usize), TransformFunc>>> = Once::new();

fn registry() -> &'static Mutex<BTreeMap<(usize, usize), TransformFunc>> {
    TRANSFORM_REGISTRY.call_once(|| Mutex::new(BTreeMap::new()))
}

/// Whether the built-in transforms have been registered yet.
static BUILTIN_REGISTERED: Mutex<bool> = Mutex::new(false);

// ────────────────────────── public API ────────────────────────────────────

/// Register a transform function (`g_value_register_transform_func`).
///
/// Any previously registered transform for `(src_type, dest_type)` is
/// replaced.
pub fn value_register_transform_func(src_type: usize, dest_type: usize, func: TransformFunc) {
    registry().lock().insert((src_type, dest_type), func);
}

/// Check whether `value_transform` can copy `src_type` into `dest_type`
/// without a registered transform (`g_value_type_compatible`).
///
/// Returns `true` if `src_type == dest_type`, or if `src_type` is a
/// subtype of `dest_type` and they share the same value table. We
/// approximate the value-table check as "same fundamental type" since
/// our type system doesn't expose value tables per-type yet.
pub fn value_type_compatible(src_type: usize, dest_type: usize) -> bool {
    if src_type == dest_type {
        return true;
    }
    // Approximate: if src is-a dest, consider them compatible. The
    // upstream additionally checks that the value tables match, but
    // our type system doesn't expose value tables per-type yet.
    type_is_a(src_type, dest_type)
}

/// Check whether `value_transform` can transform `src_type` into
/// `dest_type` (`g_value_type_transformable`).
///
/// Returns `true` if the types are compatible (see
/// `value_type_compatible`) or if a transform function is registered.
pub fn value_type_transformable(src_type: usize, dest_type: usize) -> bool {
    if value_type_compatible(src_type, dest_type) {
        return true;
    }
    registry().lock().contains_key(&(src_type, dest_type))
}

/// Transform `src` into `dest` (`g_value_transform`).
///
/// If the types are compatible (same type or subtype), copies `src`
/// into `dest`. Otherwise, looks up a registered transform function
/// and applies it. Returns `true` on success, `false` if no transform
/// is possible (in which case `dest` is not modified).
pub fn value_transform(src: &GValue, dest: &mut GValue) -> bool {
    // Ensure the built-in transforms are registered before lookups.
    ensure_builtin_transforms();

    let src_type = src.value_type();
    let dest_type = dest.value_type();

    if value_type_compatible(src_type, dest_type) {
        dest.copy_from(src);
        return true;
    }

    let reg = registry().lock();
    if let Some(&func) = reg.get(&(src_type, dest_type)) {
        // Drop the lock before calling the function (it may re-enter
        // the registry for nested transforms, though ours don't).
        drop(reg);
        func(src, dest);
        true
    } else {
        false
    }
}

// ──────────────────────── built-in transforms ─────────────────────────────

/// Register the built-in transforms between numeric types and from
/// numerics to strings. Idempotent — safe to call multiple times.
/// Called automatically by `ensure_builtin_transforms`.
pub fn init_builtin_transforms() {
    let mut registered = BUILTIN_REGISTERED.lock();
    if *registered {
        return;
    }

    // Use macros to generate plain `fn` items (closures that capture
    // `get_as_i64` can't be coerced to `fn` pointers, which
    // `TransformFunc` requires).
    macro_rules! register_int_source {
        ($src_type:expr, $get:ident) => {
            value_register_transform_func($src_type, G_TYPE_CHAR, |src, dest| {
                dest.set_char($get(src) as i8);
            });
            value_register_transform_func($src_type, G_TYPE_UCHAR, |src, dest| {
                dest.set_uchar($get(src) as u8);
            });
            value_register_transform_func($src_type, G_TYPE_INT, |src, dest| {
                dest.set_int($get(src) as i32);
            });
            value_register_transform_func($src_type, G_TYPE_UINT, |src, dest| {
                dest.set_uint($get(src) as u32);
            });
            value_register_transform_func($src_type, G_TYPE_LONG, |src, dest| {
                dest.set_long($get(src) as i64);
            });
            value_register_transform_func($src_type, G_TYPE_ULONG, |src, dest| {
                dest.set_ulong($get(src) as u64);
            });
            value_register_transform_func($src_type, G_TYPE_INT64, |src, dest| {
                dest.set_int64($get(src));
            });
            value_register_transform_func($src_type, G_TYPE_UINT64, |src, dest| {
                dest.set_uint64($get(src) as u64);
            });
            value_register_transform_func($src_type, G_TYPE_FLOAT, |src, dest| {
                dest.set_float($get(src) as f32);
            });
            value_register_transform_func($src_type, G_TYPE_DOUBLE, |src, dest| {
                dest.set_double($get(src) as f64);
            });
            value_register_transform_func($src_type, G_TYPE_BOOLEAN, |src, dest| {
                dest.set_boolean($get(src) != 0);
            });
            value_register_transform_func($src_type, G_TYPE_STRING, |src, dest| {
                dest.set_string(&format!("{}", $get(src)));
            });
        };
    }

    // Helper functions to read each integer source type as i64.
    fn get_char(v: &GValue) -> i64 {
        v.get_char() as i64
    }
    fn get_uchar(v: &GValue) -> i64 {
        v.get_uchar() as i64
    }
    fn get_int(v: &GValue) -> i64 {
        v.get_int() as i64
    }
    fn get_uint(v: &GValue) -> i64 {
        v.get_uint() as i64
    }
    fn get_long(v: &GValue) -> i64 {
        v.get_long()
    }
    fn get_ulong(v: &GValue) -> i64 {
        v.get_ulong() as i64
    }
    fn get_int64(v: &GValue) -> i64 {
        v.get_int64()
    }
    fn get_uint64(v: &GValue) -> i64 {
        v.get_uint64() as i64
    }

    register_int_source!(G_TYPE_CHAR, get_char);
    register_int_source!(G_TYPE_UCHAR, get_uchar);
    register_int_source!(G_TYPE_INT, get_int);
    register_int_source!(G_TYPE_UINT, get_uint);
    register_int_source!(G_TYPE_LONG, get_long);
    register_int_source!(G_TYPE_ULONG, get_ulong);
    register_int_source!(G_TYPE_INT64, get_int64);
    register_int_source!(G_TYPE_UINT64, get_uint64);

    // Float/double source types.
    macro_rules! register_float_source {
        ($src_type:expr, $get:ident) => {
            value_register_transform_func($src_type, G_TYPE_CHAR, |src, dest| {
                dest.set_char($get(src) as i8);
            });
            value_register_transform_func($src_type, G_TYPE_UCHAR, |src, dest| {
                dest.set_uchar($get(src) as u8);
            });
            value_register_transform_func($src_type, G_TYPE_INT, |src, dest| {
                dest.set_int($get(src) as i32);
            });
            value_register_transform_func($src_type, G_TYPE_UINT, |src, dest| {
                dest.set_uint($get(src) as u32);
            });
            value_register_transform_func($src_type, G_TYPE_LONG, |src, dest| {
                dest.set_long($get(src) as i64);
            });
            value_register_transform_func($src_type, G_TYPE_ULONG, |src, dest| {
                dest.set_ulong($get(src) as u64);
            });
            value_register_transform_func($src_type, G_TYPE_INT64, |src, dest| {
                dest.set_int64($get(src) as i64);
            });
            value_register_transform_func($src_type, G_TYPE_UINT64, |src, dest| {
                dest.set_uint64($get(src) as u64);
            });
            value_register_transform_func($src_type, G_TYPE_FLOAT, |src, dest| {
                dest.set_float($get(src) as f32);
            });
            value_register_transform_func($src_type, G_TYPE_DOUBLE, |src, dest| {
                dest.set_double($get(src));
            });
            value_register_transform_func($src_type, G_TYPE_BOOLEAN, |src, dest| {
                dest.set_boolean($get(src) != 0.0);
            });
            value_register_transform_func($src_type, G_TYPE_STRING, |src, dest| {
                dest.set_string(&format!("{}", $get(src)));
            });
        };
    }

    fn get_float(v: &GValue) -> f64 {
        v.get_float() as f64
    }
    fn get_double(v: &GValue) -> f64 {
        v.get_double()
    }

    register_float_source!(G_TYPE_FLOAT, get_float);
    register_float_source!(G_TYPE_DOUBLE, get_double);

    // Bool → string.
    value_register_transform_func(G_TYPE_BOOLEAN, G_TYPE_STRING, bool_to_string);

    *registered = true;
}

/// Ensure the built-in transforms are registered. Called by
/// `value_transform` automatically.
fn ensure_builtin_transforms() {
    let registered = *BUILTIN_REGISTERED.lock();
    if !registered {
        init_builtin_transforms();
    }
}

fn bool_to_string(src: &GValue, dest: &mut GValue) {
    dest.set_string(if src.get_boolean() { "TRUE" } else { "FALSE" });
}

// ───────────────────────────── tests ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gvalue::GValue;

    fn init() {
        init_builtin_transforms();
    }

    #[test]
    fn int_to_uint_transform() {
        init();
        let src = GValue::for_type(G_TYPE_INT);
        let mut src = src;
        src.set_int(42);
        let mut dest = GValue::for_type(G_TYPE_UINT);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_uint(), 42);
    }

    #[test]
    fn int_to_double_transform() {
        init();
        let mut src = GValue::for_type(G_TYPE_INT);
        src.set_int(42);
        let mut dest = GValue::for_type(G_TYPE_DOUBLE);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_double(), 42.0);
    }

    #[test]
    fn double_to_int_transform_truncates() {
        init();
        let mut src = GValue::for_type(G_TYPE_DOUBLE);
        src.set_double(42.7);
        let mut dest = GValue::for_type(G_TYPE_INT);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_int(), 42); // truncates toward zero
    }

    #[test]
    fn uint_to_int_transform() {
        init();
        let mut src = GValue::for_type(G_TYPE_UINT);
        src.set_uint(100);
        let mut dest = GValue::for_type(G_TYPE_INT);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_int(), 100);
    }

    #[test]
    fn int_to_bool_transform() {
        init();
        let mut src = GValue::for_type(G_TYPE_INT);
        src.set_int(1);
        let mut dest = GValue::for_type(G_TYPE_BOOLEAN);
        assert!(value_transform(&src, &mut dest));
        assert!(dest.get_boolean());

        src.set_int(0);
        assert!(value_transform(&src, &mut dest));
        assert!(!dest.get_boolean());
    }

    #[test]
    fn int_to_string_transform() {
        init();
        let mut src = GValue::for_type(G_TYPE_INT);
        src.set_int(12345);
        let mut dest = GValue::for_type(G_TYPE_STRING);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_string(), Some("12345"));
    }

    #[test]
    fn double_to_string_transform() {
        init();
        let mut src = GValue::for_type(G_TYPE_DOUBLE);
        src.set_double(3.5);
        let mut dest = GValue::for_type(G_TYPE_STRING);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_string(), Some("3.5"));
    }

    #[test]
    fn bool_to_string_transform() {
        init();
        let mut src = GValue::for_type(G_TYPE_BOOLEAN);
        src.set_boolean(true);
        let mut dest = GValue::for_type(G_TYPE_STRING);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_string(), Some("TRUE"));

        src.set_boolean(false);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_string(), Some("FALSE"));
    }

    #[test]
    fn int64_to_int_transform() {
        init();
        let mut src = GValue::for_type(G_TYPE_INT64);
        src.set_int64(1234567890);
        let mut dest = GValue::for_type(G_TYPE_INT);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_int(), 1234567890);
    }

    #[test]
    fn float_to_double_transform() {
        init();
        let mut src = GValue::for_type(G_TYPE_FLOAT);
        src.set_float(1.5);
        let mut dest = GValue::for_type(G_TYPE_DOUBLE);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_double(), 1.5);
    }

    #[test]
    fn same_type_copy_via_compatible() {
        init();
        let mut src = GValue::for_type(G_TYPE_INT);
        src.set_int(99);
        let mut dest = GValue::for_type(G_TYPE_INT);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_int(), 99);
    }

    #[test]
    fn no_transform_returns_false() {
        init();
        let mut src = GValue::for_type(G_TYPE_STRING);
        src.set_string("hello");
        let mut dest = GValue::for_type(G_TYPE_INT);
        // No registered transform from string to int.
        assert!(!value_transform(&src, &mut dest));
    }

    #[test]
    fn type_transformable_check() {
        init();
        assert!(value_type_transformable(G_TYPE_INT, G_TYPE_UINT));
        assert!(value_type_transformable(G_TYPE_INT, G_TYPE_DOUBLE));
        assert!(value_type_transformable(G_TYPE_INT, G_TYPE_STRING));
        assert!(value_type_transformable(G_TYPE_INT, G_TYPE_INT)); // same type
                                                                   // STRING→INT may be registered by the register_custom_transform
                                                                   // test (shared global registry), so we can't reliably assert
                                                                   // it's not transformable. Use a pair that no test registers.
        assert!(!value_type_transformable(G_TYPE_STRING, G_TYPE_FLOAT));
    }

    #[test]
    fn type_compatible_check() {
        init();
        assert!(value_type_compatible(G_TYPE_INT, G_TYPE_INT));
        assert!(!value_type_compatible(G_TYPE_INT, G_TYPE_UINT));
    }

    #[test]
    fn register_custom_transform() {
        init();
        // Register a custom transform: string → int (parse).
        fn parse_int(src: &GValue, dest: &mut GValue) {
            if let Some(s) = src.get_string() {
                if let Ok(n) = s.parse::<i32>() {
                    dest.set_int(n);
                }
            }
        }
        value_register_transform_func(G_TYPE_STRING, G_TYPE_INT, parse_int);

        let mut src = GValue::for_type(G_TYPE_STRING);
        src.set_string("42");
        let mut dest = GValue::for_type(G_TYPE_INT);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_int(), 42);
    }

    #[test]
    fn init_builtin_is_idempotent() {
        init_builtin_transforms();
        init_builtin_transforms(); // should not panic or double-register
                                   // Verify transforms still work.
        let mut src = GValue::for_type(G_TYPE_INT);
        src.set_int(7);
        let mut dest = GValue::for_type(G_TYPE_UINT);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_uint(), 7);
    }

    #[test]
    fn char_to_int_transform() {
        init();
        let mut src = GValue::for_type(G_TYPE_CHAR);
        src.set_char(65); // 'A'
        let mut dest = GValue::for_type(G_TYPE_INT);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_int(), 65);
    }

    #[test]
    fn uint64_to_double_transform() {
        init();
        let mut src = GValue::for_type(G_TYPE_UINT64);
        src.set_uint64(1_000_000);
        let mut dest = GValue::for_type(G_TYPE_DOUBLE);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_double(), 1_000_000.0);
    }

    #[test]
    fn long_to_string_transform() {
        init();
        let mut src = GValue::for_type(G_TYPE_LONG);
        src.set_long(9876543210);
        let mut dest = GValue::for_type(G_TYPE_STRING);
        assert!(value_transform(&src, &mut dest));
        assert_eq!(dest.get_string(), Some("9876543210"));
    }
}
