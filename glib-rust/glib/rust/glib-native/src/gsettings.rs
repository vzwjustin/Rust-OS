//! GSettings key-value store matching `gio/gsettings.h` / `gio/gsettings.c`.
//!
//! Upstream `GSettings` is a `GObject` subclass that provides a typed,
//! schema-validated key-value store backed by a configurable backend.
//! For bare-metal `no_std` we port it as a `BTreeMap`-backed struct wrapped
//! in a `Mutex`, retaining the typed value API while dropping backend I/O
//! and schema XML parsing.
//!
//! Provides:
//! - `SettingsValue` enum for all GSettings value types.
//! - `Settings` struct with get/set/reset/list_keys methods.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A typed value that can be stored in `Settings`.
///
/// Mirrors the GVariant types used by upstream `GSettings` (`b`, `i`, `x`,
/// `u`, `t`, `d`, `s`, `as`).
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsValue {
    /// Boolean value (`b`).
    Bool(bool),
    /// 32-bit signed integer (`i`).
    Int(i32),
    /// 64-bit signed integer (`x`).
    Int64(i64),
    /// 32-bit unsigned integer (`u`).
    Uint(u32),
    /// 64-bit unsigned integer (`t`).
    Uint64(u64),
    /// Double-precision float (`d`).
    Double(f64),
    /// UTF-8 string (`s`).
    Str(String),
    /// Array of UTF-8 strings (`as`).
    Strv(Vec<String>),
}

/// An in-memory key-value store matching the `GSettings` API.
///
/// Keys are arbitrary `String`s; in upstream GSettings they would be
/// validated against a schema, but here validation is deferred (bare-metal
/// has no schema registry).
pub struct Settings {
    schema_id: String,
    map: Mutex<BTreeMap<String, SettingsValue>>,
}

impl Settings {
    /// Creates a new, empty `Settings` object for the given schema ID.
    ///
    /// Mirrors `g_settings_new`.
    pub fn new(schema_id: &str) -> Self {
        Self {
            schema_id: schema_id.to_string(),
            map: Mutex::new(BTreeMap::new()),
        }
    }

    /// Returns the schema ID this `Settings` object was constructed with.
    ///
    /// Mirrors `g_settings_get_schema_id`.
    pub fn get_schema_id(&self) -> &str {
        &self.schema_id
    }

    // ── Boolean ─────────────────────────────────────────────────────────────

    /// Gets a boolean value.
    ///
    /// Returns `false` if the key is absent or has a non-boolean type.
    /// Mirrors `g_settings_get_boolean`.
    pub fn get_boolean(&self, key: &str) -> bool {
        match self.map.lock().get(key) {
            Some(SettingsValue::Bool(v)) => *v,
            _ => false,
        }
    }

    /// Sets a boolean value.
    ///
    /// Mirrors `g_settings_set_boolean`.
    pub fn set_boolean(&self, key: &str, val: bool) {
        self.map
            .lock()
            .insert(key.to_string(), SettingsValue::Bool(val));
    }

    // ── Int (i32) ────────────────────────────────────────────────────────────

    /// Gets a 32-bit signed integer value.
    ///
    /// Returns `0` if the key is absent or has an incompatible type.
    /// Mirrors `g_settings_get_int`.
    pub fn get_int(&self, key: &str) -> i32 {
        match self.map.lock().get(key) {
            Some(SettingsValue::Int(v)) => *v,
            _ => 0,
        }
    }

    /// Sets a 32-bit signed integer value.
    ///
    /// Mirrors `g_settings_set_int`.
    pub fn set_int(&self, key: &str, val: i32) {
        self.map
            .lock()
            .insert(key.to_string(), SettingsValue::Int(val));
    }

    // ── Uint (u32) ───────────────────────────────────────────────────────────

    /// Gets a 32-bit unsigned integer value.
    ///
    /// Returns `0` if the key is absent or has an incompatible type.
    /// Mirrors `g_settings_get_uint`.
    pub fn get_uint(&self, key: &str) -> u32 {
        match self.map.lock().get(key) {
            Some(SettingsValue::Uint(v)) => *v,
            _ => 0,
        }
    }

    /// Sets a 32-bit unsigned integer value.
    ///
    /// Mirrors `g_settings_set_uint`.
    pub fn set_uint(&self, key: &str, val: u32) {
        self.map
            .lock()
            .insert(key.to_string(), SettingsValue::Uint(val));
    }

    // ── Int64 (i64) ──────────────────────────────────────────────────────────

    /// Gets a 64-bit signed integer value.
    ///
    /// Returns `0` if the key is absent or has an incompatible type.
    /// Mirrors `g_settings_get_int64`.
    pub fn get_int64(&self, key: &str) -> i64 {
        match self.map.lock().get(key) {
            Some(SettingsValue::Int64(v)) => *v,
            _ => 0,
        }
    }

    /// Sets a 64-bit signed integer value.
    ///
    /// Mirrors `g_settings_set_int64`.
    pub fn set_int64(&self, key: &str, val: i64) {
        self.map
            .lock()
            .insert(key.to_string(), SettingsValue::Int64(val));
    }

    // ── String ───────────────────────────────────────────────────────────────

    /// Gets a UTF-8 string value.
    ///
    /// Returns an empty `String` if the key is absent or has an incompatible
    /// type. Mirrors `g_settings_get_string`.
    pub fn get_string(&self, key: &str) -> String {
        match self.map.lock().get(key) {
            Some(SettingsValue::Str(v)) => v.clone(),
            _ => String::new(),
        }
    }

    /// Sets a UTF-8 string value.
    ///
    /// Mirrors `g_settings_set_string`.
    pub fn set_string(&self, key: &str, val: &str) {
        self.map
            .lock()
            .insert(key.to_string(), SettingsValue::Str(val.to_string()));
    }

    // ── Strv ─────────────────────────────────────────────────────────────────

    /// Gets a string-array value.
    ///
    /// Returns an empty `Vec` if the key is absent or has an incompatible
    /// type. Mirrors `g_settings_get_strv`.
    pub fn get_strv(&self, key: &str) -> Vec<String> {
        match self.map.lock().get(key) {
            Some(SettingsValue::Strv(v)) => v.clone(),
            _ => Vec::new(),
        }
    }

    /// Sets a string-array value.
    ///
    /// Mirrors `g_settings_set_strv`.
    pub fn set_strv(&self, key: &str, val: Vec<String>) {
        self.map
            .lock()
            .insert(key.to_string(), SettingsValue::Strv(val));
    }

    // ── Reset / enumerate ────────────────────────────────────────────────────

    /// Resets a key to the default (removes it from the in-memory map).
    ///
    /// Mirrors `g_settings_reset`.
    pub fn reset(&self, key: &str) {
        self.map.lock().remove(key);
    }

    /// Returns a sorted list of all keys currently stored.
    ///
    /// Mirrors `g_settings_list_keys`.
    pub fn list_keys(&self) -> Vec<String> {
        self.map.lock().keys().cloned().collect()
    }
}

// ──────────────────────────── Tests ────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make() -> Settings {
        Settings::new("org.gnome.Example")
    }

    #[test]
    fn test_schema_id() {
        let s = make();
        assert_eq!(s.get_schema_id(), "org.gnome.Example");
    }

    #[test]
    fn test_boolean_roundtrip() {
        let s = make();
        assert!(!s.get_boolean("flag"));
        s.set_boolean("flag", true);
        assert!(s.get_boolean("flag"));
        s.set_boolean("flag", false);
        assert!(!s.get_boolean("flag"));
    }

    #[test]
    fn test_int_roundtrip() {
        let s = make();
        assert_eq!(s.get_int("count"), 0);
        s.set_int("count", -42);
        assert_eq!(s.get_int("count"), -42);
    }

    #[test]
    fn test_uint_roundtrip() {
        let s = make();
        assert_eq!(s.get_uint("size"), 0);
        s.set_uint("size", 1024);
        assert_eq!(s.get_uint("size"), 1024);
    }

    #[test]
    fn test_int64_roundtrip() {
        let s = make();
        assert_eq!(s.get_int64("ts"), 0);
        s.set_int64("ts", i64::MIN);
        assert_eq!(s.get_int64("ts"), i64::MIN);
    }

    #[test]
    fn test_string_roundtrip() {
        let s = make();
        assert_eq!(s.get_string("name"), "");
        s.set_string("name", "hello");
        assert_eq!(s.get_string("name"), "hello");
    }

    #[test]
    fn test_strv_roundtrip() {
        let s = make();
        assert!(s.get_strv("paths").is_empty());
        s.set_strv("paths", alloc::vec!["/usr".to_string(), "/bin".to_string()]);
        let got = s.get_strv("paths");
        assert_eq!(got, &["/usr", "/bin"]);
    }

    #[test]
    fn test_reset_removes_key() {
        let s = make();
        s.set_int("x", 99);
        assert_eq!(s.get_int("x"), 99);
        s.reset("x");
        assert_eq!(s.get_int("x"), 0);
    }

    #[test]
    fn test_list_keys_sorted() {
        let s = make();
        s.set_boolean("zebra", true);
        s.set_int("alpha", 1);
        s.set_string("mango", "yes");
        let keys = s.list_keys();
        assert_eq!(keys, &["alpha", "mango", "zebra"]);
    }

    #[test]
    fn test_type_mismatch_returns_default() {
        let s = make();
        // Store as boolean, read as int — should get default 0.
        s.set_boolean("mixed", true);
        assert_eq!(s.get_int("mixed"), 0);
        assert_eq!(s.get_string("mixed"), "");
        assert!(s.get_strv("mixed").is_empty());
    }
}
