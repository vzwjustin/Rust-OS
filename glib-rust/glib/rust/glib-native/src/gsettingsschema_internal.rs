//! `gsettingsschema-internal` matching `gio/gsettingsschema-internal.h`.
//!
//! Internal settings schema helpers adapted to the current string-backed
//! `SettingsSchema` representation.

use crate::gsettingsschema::{SettingsSchema, SettingsSchemaKey};
use crate::variant::Variant;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Returns gettext domain for schema.
pub fn schema_get_gettext_domain(_schema: &SettingsSchema) -> &str {
    ""
}

/// Gets a typed default value from schema.
pub fn schema_get_value(schema: &SettingsSchema, key: &str) -> Option<Variant> {
    schema.get_key(key).map(default_variant_for_key)
}

/// Lists all keys in schema.
pub fn schema_list(schema: &SettingsSchema) -> Vec<String> {
    schema.list_keys()
}

/// Gets string default value from schema.
pub fn schema_get_string(schema: &SettingsSchema, key: &str) -> Option<String> {
    schema
        .get_key(key)
        .map(|k| k.get_default_value().to_string())
}

/// Gets child schema.
pub fn schema_get_child_schema(_schema: &SettingsSchema, _name: &str) -> Option<SettingsSchema> {
    None
}

/// Initializes a schema key.
pub fn schema_key_init(schema: &SettingsSchema, name: &str) -> Option<SettingsSchemaKey> {
    schema.get_key(name).cloned()
}

/// Type-checks a value against a key.
pub fn schema_key_type_check(key: &SettingsSchemaKey, value: &Variant) -> bool {
    value.type_string() == key.get_value_type()
}

/// Range fixup. The current key model has no range metadata, so this only
/// rejects type mismatches.
pub fn schema_key_range_fixup(key: &SettingsSchemaKey, value: &Variant) -> Option<Variant> {
    schema_key_type_check(key, value).then(|| value.clone())
}

/// Gets default value for a key.
pub fn schema_key_get_default_value(key: &SettingsSchemaKey) -> Variant {
    default_variant_for_key(key)
}

/// Gets translated default value for a key.
pub fn schema_key_get_translated_default(key: &SettingsSchemaKey) -> Variant {
    default_variant_for_key(key)
}

/// Gets per-desktop default value for a key.
pub fn schema_key_get_per_desktop_default(_key: &SettingsSchemaKey) -> Option<Variant> {
    None
}

/// Converts a variant to an enum value.
pub fn schema_key_to_enum(_key: &SettingsSchemaKey, value: &Variant) -> i32 {
    value.get_int32()
}

/// Converts an enum value to a variant.
pub fn schema_key_from_enum(_key: &SettingsSchemaKey, value: i32) -> Variant {
    Variant::new_int32(value)
}

/// Converts variant flags.
pub fn schema_key_to_flags(_key: &SettingsSchemaKey, value: &Variant) -> u32 {
    value.get_uint32()
}

/// Converts flags to a variant.
pub fn schema_key_from_flags(_key: &SettingsSchemaKey, value: u32) -> Variant {
    Variant::new_uint32(value)
}

fn default_variant_for_key(key: &SettingsSchemaKey) -> Variant {
    let value = key.get_default_value();
    match key.get_value_type() {
        "b" => Variant::new_boolean(matches!(value, "true" | "1")),
        "i" => Variant::new_int32(value.parse::<i32>().unwrap_or_default()),
        "u" => Variant::new_uint32(value.parse::<u32>().unwrap_or_default()),
        "s" => Variant::new_string(value.trim_matches('\'')),
        "o" => Variant::new_object_path(value),
        "g" => Variant::new_signature(value),
        _ => Variant::new_string(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_check() {
        let key = SettingsSchemaKey::new("test", "i", "0");
        assert!(schema_key_type_check(&key, &Variant::new_int32(42)));
        assert!(!schema_key_type_check(&key, &Variant::new_string("hello")));
    }

    #[test]
    fn test_range_fixup() {
        let key = SettingsSchemaKey::new("test", "i", "50");
        let fixed = schema_key_range_fixup(&key, &Variant::new_int32(150));
        assert_eq!(fixed.unwrap().get_int32(), 150);
        assert!(schema_key_range_fixup(&key, &Variant::new_string("bad")).is_none());
    }
}
