//! gsettings_mapping matching `gio/gsettings-mapping.c`.
//!
//! Provides mapping functions between `GValue` and `GVariant` for GSettings
//! bindings. These functions convert between GValue types (int, uint, double,
//! string, boolean, etc.) and GVariant types (int16, uint16, int32, uint32,
//! int64, uint64, double, string, boolean, byte, etc.).
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gtype::{
    G_TYPE_BOOLEAN, G_TYPE_CHAR, G_TYPE_DOUBLE, G_TYPE_INT, G_TYPE_INT64, G_TYPE_STRING,
    G_TYPE_UCHAR, G_TYPE_UINT, G_TYPE_UINT64,
};
use crate::gvalue::GValue;
use crate::variant::Variant;
use crate::varianttype::VariantType;

/// Maps a `GValue` to a `GVariant` for setting (GValue → GVariant).
///
/// Mirrors `g_settings_set_mapping`.
pub fn g_settings_set_mapping(value: &GValue, expected_type: &VariantType) -> Option<Variant> {
    let type_id = value.value_type();

    if type_id == G_TYPE_BOOLEAN {
        if expected_type.type_string() == "b" {
            return Some(Variant::new_boolean(value.get_boolean()));
        }
    } else if type_id == G_TYPE_CHAR || type_id == G_TYPE_UCHAR {
        if expected_type.type_string() == "y" {
            if type_id == G_TYPE_CHAR {
                return Some(Variant::new_byte(value.get_char() as u8));
            } else {
                return Some(Variant::new_byte(value.get_uchar()));
            }
        }
    } else if type_id == G_TYPE_INT || type_id == G_TYPE_INT64 {
        return set_mapping_int(value, expected_type);
    } else if type_id == G_TYPE_DOUBLE {
        return set_mapping_double(value, expected_type);
    } else if type_id == G_TYPE_UINT || type_id == G_TYPE_UINT64 {
        return set_mapping_unsigned_int(value, expected_type);
    } else if type_id == G_TYPE_STRING {
        if let Some(s) = value.get_string() {
            match expected_type.type_string() {
                "s" => return Some(Variant::new_string(s)),
                "ay" => return Some(Variant::new_string(s)),
                "o" => return Some(Variant::new_object_path(s)),
                "g" => return Some(Variant::new_signature(s)),
                _ => {}
            }
        }
        return None;
    }

    None
}

/// Maps a `GVariant` to a `GValue` for getting (GVariant → GValue).
///
/// Mirrors `g_settings_get_mapping`.
pub fn g_settings_get_mapping(value: &mut GValue, variant: &Variant) -> bool {
    let type_str = variant.type_string();

    if type_str == "b" {
        if value.value_type() != G_TYPE_BOOLEAN {
            return false;
        }
        value.set_boolean(variant.get_boolean());
        return true;
    }

    if type_str == "y" {
        if value.value_type() == G_TYPE_UCHAR {
            value.set_uchar(variant.get_byte());
            return true;
        } else if value.value_type() == G_TYPE_CHAR {
            value.set_char(variant.get_byte() as i8);
            return true;
        }
        return false;
    }

    if type_str == "n" || type_str == "i" || type_str == "x" || type_str == "h" {
        return get_mapping_int(value, variant);
    }

    if type_str == "d" {
        return get_mapping_double(value, variant);
    }

    if type_str == "q" || type_str == "u" || type_str == "t" {
        return get_mapping_unsigned_int(value, variant);
    }

    if type_str == "s" || type_str == "o" || type_str == "g" {
        if value.value_type() == G_TYPE_STRING {
            let s = variant.get_string();
            if !s.is_empty() || variant.classify() == crate::varianttype::VariantClass::String {
                value.set_string(s);
                return true;
            }
        }
        return false;
    }

    if type_str == "ay" {
        let s = variant.get_string();
        if !s.is_empty() {
            value.set_string(s);
            return true;
        }
        return false;
    }

    false
}

/// Checks if a GValue type and GVariant type are compatible for binding.
///
/// Mirrors `g_settings_mapping_is_compatible`.
pub fn g_settings_mapping_is_compatible(gvalue_type: usize, variant_type: &VariantType) -> bool {
    let vs = variant_type.type_string();

    if gvalue_type == G_TYPE_BOOLEAN {
        vs == "b"
    } else if gvalue_type == G_TYPE_CHAR || gvalue_type == G_TYPE_UCHAR {
        vs == "y"
    } else if gvalue_type == G_TYPE_INT
        || gvalue_type == G_TYPE_UINT
        || gvalue_type == G_TYPE_INT64
        || gvalue_type == G_TYPE_UINT64
        || gvalue_type == G_TYPE_DOUBLE
    {
        vs == "n"
            || vs == "q"
            || vs == "i"
            || vs == "u"
            || vs == "x"
            || vs == "t"
            || vs == "h"
            || vs == "d"
    } else if gvalue_type == G_TYPE_STRING {
        vs == "s" || vs == "ay" || vs == "o" || vs == "g"
    } else {
        false
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────

fn set_mapping_int(value: &GValue, expected_type: &VariantType) -> Option<Variant> {
    let l: i64 = if value.value_type() == G_TYPE_INT {
        value.get_int() as i64
    } else {
        value.get_int64()
    };

    match expected_type.type_string() {
        "n" => {
            if (i16::MIN as i64) <= l && l <= (i16::MAX as i64) {
                Some(Variant::new_int16(l as i16))
            } else {
                None
            }
        }
        "q" => {
            if 0 <= l && l <= (u16::MAX as i64) {
                Some(Variant::new_uint16(l as u16))
            } else {
                None
            }
        }
        "i" => {
            if (i32::MIN as i64) <= l && l <= (i32::MAX as i64) {
                Some(Variant::new_int32(l as i32))
            } else {
                None
            }
        }
        "u" => {
            if 0 <= l && l <= (u32::MAX as i64) {
                Some(Variant::new_uint32(l as u32))
            } else {
                None
            }
        }
        "x" => Some(Variant::new_int64(l)),
        "t" => {
            if 0 <= l {
                Some(Variant::new_uint64(l as u64))
            } else {
                None
            }
        }
        "h" => {
            if 0 <= l && l <= (u32::MAX as i64) {
                Some(Variant::new_handle(l as i32))
            } else {
                None
            }
        }
        "d" => Some(Variant::new_double(l as f64)),
        _ => None,
    }
}

fn set_mapping_double(value: &GValue, expected_type: &VariantType) -> Option<Variant> {
    let d = value.get_double();
    let l = d as i64;

    match expected_type.type_string() {
        "n" => {
            if (i16::MIN as i64) <= l && l <= (i16::MAX as i64) {
                Some(Variant::new_int16(l as i16))
            } else {
                None
            }
        }
        "q" => {
            if 0 <= l && l <= (u16::MAX as i64) {
                Some(Variant::new_uint16(l as u16))
            } else {
                None
            }
        }
        "i" => {
            if (i32::MIN as i64) <= l && l <= (i32::MAX as i64) {
                Some(Variant::new_int32(l as i32))
            } else {
                None
            }
        }
        "u" => {
            if 0 <= l && l <= (u32::MAX as i64) {
                Some(Variant::new_uint32(l as u32))
            } else {
                None
            }
        }
        "x" => Some(Variant::new_int64(l)),
        "t" => {
            if 0 <= l {
                Some(Variant::new_uint64(l as u64))
            } else {
                None
            }
        }
        "h" => {
            if 0 <= l && l <= (u32::MAX as i64) {
                Some(Variant::new_handle(l as i32))
            } else {
                None
            }
        }
        "d" => Some(Variant::new_double(d)),
        _ => None,
    }
}

fn set_mapping_unsigned_int(value: &GValue, expected_type: &VariantType) -> Option<Variant> {
    let u: u64 = if value.value_type() == G_TYPE_UINT {
        value.get_uint() as u64
    } else {
        value.get_uint64()
    };

    match expected_type.type_string() {
        "n" => {
            if u <= (i16::MAX as u64) {
                Some(Variant::new_int16(u as i16))
            } else {
                None
            }
        }
        "q" => {
            if u <= (u16::MAX as u64) {
                Some(Variant::new_uint16(u as u16))
            } else {
                None
            }
        }
        "i" => {
            if u <= (i32::MAX as u64) {
                Some(Variant::new_int32(u as i32))
            } else {
                None
            }
        }
        "u" => {
            if u <= (u32::MAX as u64) {
                Some(Variant::new_uint32(u as u32))
            } else {
                None
            }
        }
        "x" => {
            if u <= (i64::MAX as u64) {
                Some(Variant::new_int64(u as i64))
            } else {
                None
            }
        }
        "t" => Some(Variant::new_uint64(u)),
        "h" => {
            if u <= (u32::MAX as u64) {
                Some(Variant::new_handle(u as i32))
            } else {
                None
            }
        }
        "d" => Some(Variant::new_double(u as f64)),
        _ => None,
    }
}

fn get_mapping_int(value: &mut GValue, variant: &Variant) -> bool {
    let l: i64 = match variant.type_string() {
        "n" => variant.get_int16() as i64,
        "i" => variant.get_int32() as i64,
        "x" => variant.get_int64(),
        "h" => variant.get_handle() as i64,
        _ => return false,
    };

    let type_id = value.value_type();
    if type_id == G_TYPE_INT {
        value.set_int(l as i32);
        return (i32::MIN as i64) <= l && l <= (i32::MAX as i64);
    } else if type_id == G_TYPE_UINT {
        value.set_uint(l as u32);
        return 0 <= l && l <= (u32::MAX as i64);
    } else if type_id == G_TYPE_INT64 {
        value.set_int64(l);
        return true;
    } else if type_id == G_TYPE_UINT64 {
        value.set_uint64(l as u64);
        return 0 <= l;
    } else if type_id == G_TYPE_DOUBLE {
        value.set_double(l as f64);
        return true;
    }
    false
}

fn get_mapping_double(value: &mut GValue, variant: &Variant) -> bool {
    let d = variant.get_double();
    let l = d as i64;

    let type_id = value.value_type();
    if type_id == G_TYPE_INT {
        value.set_int(l as i32);
        return (i32::MIN as i64) <= l && l <= (i32::MAX as i64);
    } else if type_id == G_TYPE_UINT {
        value.set_uint(l as u32);
        return 0 <= l && l <= (u32::MAX as i64);
    } else if type_id == G_TYPE_INT64 {
        value.set_int64(l);
        return true;
    } else if type_id == G_TYPE_UINT64 {
        value.set_uint64(l as u64);
        return 0 <= l;
    } else if type_id == G_TYPE_DOUBLE {
        value.set_double(d);
        return true;
    }
    false
}

fn get_mapping_unsigned_int(value: &mut GValue, variant: &Variant) -> bool {
    let u: u64 = match variant.type_string() {
        "q" => variant.get_uint16() as u64,
        "u" => variant.get_uint32() as u64,
        "t" => variant.get_uint64(),
        _ => return false,
    };

    let type_id = value.value_type();
    if type_id == G_TYPE_INT {
        value.set_int(u as i32);
        return u <= (i32::MAX as u64);
    } else if type_id == G_TYPE_UINT {
        value.set_uint(u as u32);
        return u <= (u32::MAX as u64);
    } else if type_id == G_TYPE_INT64 {
        value.set_int64(u as i64);
        return u <= (i64::MAX as u64);
    } else if type_id == G_TYPE_UINT64 {
        value.set_uint64(u);
        return true;
    } else if type_id == G_TYPE_DOUBLE {
        value.set_double(u as f64);
        return true;
    }
    false
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtype::type_init;
    use crate::gvalue::GValue;

    #[test]
    fn test_boolean_mapping() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_BOOLEAN);
        v.set_boolean(true);
        let vt = VariantType::new("b").unwrap();
        let variant = g_settings_set_mapping(&v, &vt).unwrap();
        assert!(variant.get_boolean());

        let mut v2 = GValue::for_type(G_TYPE_BOOLEAN);
        assert!(g_settings_get_mapping(&mut v2, &variant));
        assert!(v2.get_boolean());
    }

    #[test]
    fn test_int_to_int32_mapping() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_INT);
        v.set_int(42);
        let vt = VariantType::new("i").unwrap();
        let variant = g_settings_set_mapping(&v, &vt).unwrap();
        assert_eq!(variant.get_int32(), 42);

        let mut v2 = GValue::for_type(G_TYPE_INT);
        assert!(g_settings_get_mapping(&mut v2, &variant));
        assert_eq!(v2.get_int(), 42);
    }

    #[test]
    fn test_string_mapping() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_STRING);
        v.set_string("hello");
        let vt = VariantType::new("s").unwrap();
        let variant = g_settings_set_mapping(&v, &vt).unwrap();
        assert_eq!(variant.get_string(), "hello");

        let mut v2 = GValue::for_type(G_TYPE_STRING);
        assert!(g_settings_get_mapping(&mut v2, &variant));
        assert_eq!(v2.get_string(), Some("hello"));
    }

    #[test]
    fn test_double_mapping() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_DOUBLE);
        v.set_double(3.14);
        let vt = VariantType::new("d").unwrap();
        let variant = g_settings_set_mapping(&v, &vt).unwrap();
        assert!((variant.get_double() - 3.14).abs() < f64::EPSILON);

        let mut v2 = GValue::for_type(G_TYPE_DOUBLE);
        assert!(g_settings_get_mapping(&mut v2, &variant));
        assert!((v2.get_double() - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compatibility() {
        type_init();
        assert!(g_settings_mapping_is_compatible(
            G_TYPE_BOOLEAN,
            &VariantType::new("b").unwrap()
        ));
        assert!(g_settings_mapping_is_compatible(
            G_TYPE_INT,
            &VariantType::new("i").unwrap()
        ));
        assert!(g_settings_mapping_is_compatible(
            G_TYPE_STRING,
            &VariantType::new("s").unwrap()
        ));
        assert!(!g_settings_mapping_is_compatible(
            G_TYPE_BOOLEAN,
            &VariantType::new("s").unwrap()
        ));
        assert!(!g_settings_mapping_is_compatible(
            G_TYPE_INT,
            &VariantType::new("b").unwrap()
        ));
    }

    #[test]
    fn test_int_range_check() {
        type_init();
        let mut v = GValue::for_type(G_TYPE_INT);
        v.set_int(300);
        let vt = VariantType::new("n").unwrap();
        // 300 > i16::MAX (32767), wait no, 300 < 32767
        assert!(g_settings_set_mapping(&v, &vt).is_some());

        v.set_int(40000);
        // 40000 > i16::MAX
        assert!(g_settings_set_mapping(&v, &vt).is_none());
    }
}
