//! Enum and flags type registration (`genums.c`).
//!
//! This module mirrors the public registration and lookup surface for
//! `GEnumValue` / `GFlagsValue` while using the Rust `GType` registry.

use crate::gtype::{
    type_from_name, type_register_static, GType, GTypeFlags, GTypeInfo, G_TYPE_ENUM, G_TYPE_FLAGS,
    G_TYPE_INVALID,
};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::{Mutex, Once};

/// One enum value (`GEnumValue`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumValue {
    /// Numeric enum value.
    pub value: i32,
    /// Canonical value name.
    pub value_name: String,
    /// Short nickname.
    pub value_nick: String,
}

impl EnumValue {
    /// Create an enum value descriptor.
    #[must_use]
    pub fn new(value: i32, value_name: &str, value_nick: &str) -> Self {
        Self {
            value,
            value_name: String::from(value_name),
            value_nick: String::from(value_nick),
        }
    }
}

/// One flags value (`GFlagsValue`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagsValue {
    /// Numeric bit value.
    pub value: u32,
    /// Canonical value name.
    pub value_name: String,
    /// Short nickname.
    pub value_nick: String,
}

impl FlagsValue {
    /// Create a flags value descriptor.
    #[must_use]
    pub fn new(value: u32, value_name: &str, value_nick: &str) -> Self {
        Self {
            value,
            value_name: String::from(value_name),
            value_nick: String::from(value_nick),
        }
    }
}

static ENUM_VALUES: Once<Mutex<BTreeMap<GType, Vec<EnumValue>>>> = Once::new();
static FLAGS_VALUES: Once<Mutex<BTreeMap<GType, Vec<FlagsValue>>>> = Once::new();

fn enum_values() -> &'static Mutex<BTreeMap<GType, Vec<EnumValue>>> {
    ENUM_VALUES.call_once(|| Mutex::new(BTreeMap::new()))
}

fn flags_values() -> &'static Mutex<BTreeMap<GType, Vec<FlagsValue>>> {
    FLAGS_VALUES.call_once(|| Mutex::new(BTreeMap::new()))
}

/// Register a static enum type (`g_enum_register_static`).
pub fn enum_register_static(name: &str, values: &[EnumValue]) -> GType {
    if name.is_empty() || values.is_empty() {
        return 0;
    }

    let existing = type_from_name(name);
    if existing != 0 {
        return existing;
    }

    let type_id = type_register_static(G_TYPE_ENUM, name, &GTypeInfo::default(), GTypeFlags::NONE);
    if type_id != 0 {
        enum_values().lock().insert(type_id, values.to_vec());
    }
    type_id
}

/// Register a static flags type (`g_flags_register_static`).
pub fn flags_register_static(name: &str, values: &[FlagsValue]) -> GType {
    if name.is_empty() || values.is_empty() {
        return 0;
    }

    let existing = type_from_name(name);
    if existing != 0 {
        return existing;
    }

    let type_id = type_register_static(G_TYPE_FLAGS, name, &GTypeInfo::default(), GTypeFlags::NONE);
    if type_id != 0 {
        flags_values().lock().insert(type_id, values.to_vec());
    }
    type_id
}

/// Find enum value by integer (`g_enum_get_value`).
#[must_use]
pub fn enum_get_value(type_id: GType, value: i32) -> Option<EnumValue> {
    enum_values()
        .lock()
        .get(&type_id)
        .and_then(|values| values.iter().find(|entry| entry.value == value).cloned())
}

/// Find enum value by canonical name (`g_enum_get_value_by_name`).
#[must_use]
pub fn enum_get_value_by_name(type_id: GType, name: &str) -> Option<EnumValue> {
    enum_values().lock().get(&type_id).and_then(|values| {
        values
            .iter()
            .find(|entry| entry.value_name == name)
            .cloned()
    })
}

/// Find enum value by nickname (`g_enum_get_value_by_nick`).
#[must_use]
pub fn enum_get_value_by_nick(type_id: GType, nick: &str) -> Option<EnumValue> {
    enum_values().lock().get(&type_id).and_then(|values| {
        values
            .iter()
            .find(|entry| entry.value_nick == nick)
            .cloned()
    })
}

/// Find flags value by bit value (`g_flags_get_first_value`).
#[must_use]
pub fn flags_get_first_value(type_id: GType, value: u32) -> Option<FlagsValue> {
    flags_values()
        .lock()
        .get(&type_id)
        .and_then(|values| values.iter().find(|entry| entry.value == value).cloned())
}

/// Find flags value by canonical name (`g_flags_get_value_by_name`).
#[must_use]
pub fn flags_get_value_by_name(type_id: GType, name: &str) -> Option<FlagsValue> {
    flags_values().lock().get(&type_id).and_then(|values| {
        values
            .iter()
            .find(|entry| entry.value_name == name)
            .cloned()
    })
}

/// Find flags value by nickname (`g_flags_get_value_by_nick`).
#[must_use]
pub fn flags_get_value_by_nick(type_id: GType, nick: &str) -> Option<FlagsValue> {
    flags_values().lock().get(&type_id).and_then(|values| {
        values
            .iter()
            .find(|entry| entry.value_nick == nick)
            .cloned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtype::{type_is_a, type_name};

    #[test]
    fn registers_enum_and_looks_up_values() {
        let values = [
            EnumValue::new(0, "RUST_NATIVE_ZERO", "zero"),
            EnumValue::new(7, "RUST_NATIVE_SEVEN", "seven"),
        ];
        let type_id = enum_register_static("RustNativeEnum", &values);

        assert_ne!(type_id, 0);
        assert!(type_is_a(type_id, G_TYPE_ENUM));
        assert_eq!(type_name(type_id).as_deref(), Some("RustNativeEnum"));
        assert_eq!(enum_get_value(type_id, 7), Some(values[1].clone()));
        assert_eq!(
            enum_get_value_by_name(type_id, "RUST_NATIVE_ZERO"),
            Some(values[0].clone())
        );
        assert_eq!(
            enum_get_value_by_nick(type_id, "seven"),
            Some(values[1].clone())
        );
    }

    #[test]
    fn registers_flags_and_looks_up_values() {
        let values = [
            FlagsValue::new(1, "RUST_NATIVE_READ", "read"),
            FlagsValue::new(2, "RUST_NATIVE_WRITE", "write"),
        ];
        let type_id = flags_register_static("RustNativeFlags", &values);

        assert_ne!(type_id, 0);
        assert!(type_is_a(type_id, G_TYPE_FLAGS));
        assert_eq!(flags_get_first_value(type_id, 2), Some(values[1].clone()));
        assert_eq!(
            flags_get_value_by_name(type_id, "RUST_NATIVE_READ"),
            Some(values[0].clone())
        );
        assert_eq!(
            flags_get_value_by_nick(type_id, "write"),
            Some(values[1].clone())
        );
    }

    #[test]
    fn duplicate_enum_name_returns_existing_type() {
        let values = [EnumValue::new(1, "RUST_NATIVE_DUP", "dup")];
        let first = enum_register_static("RustNativeEnumDup", &values);
        let second = enum_register_static("RustNativeEnumDup", &values);

        assert_eq!(first, second);
    }
}
