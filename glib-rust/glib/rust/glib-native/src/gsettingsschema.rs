//! GSettingsSchema matching `gio/gsettingsschema.h`.
//!
//! Upstream `GSettingsSchema` provides schema lookup for GSettings.
//! We port `GSettingsSchemaSource`, `GSettingsSchema`, and `GSettingsSchemaKey`
//! as plain Rust structs with an in-memory registry.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A settings schema key (`GSettingsSchemaKey`).
#[derive(Clone, Debug)]
pub struct SettingsSchemaKey {
    name: String,
    type_string: String,
    default_value: String,
    summary: String,
    description: String,
}

impl SettingsSchemaKey {
    pub fn new(name: &str, type_string: &str, default_value: &str) -> Self {
        Self {
            name: name.to_string(),
            type_string: type_string.to_string(),
            default_value: default_value.to_string(),
            summary: String::new(),
            description: String::new(),
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_value_type(&self) -> &str {
        &self.type_string
    }

    pub fn get_default_value(&self) -> &str {
        &self.default_value
    }

    pub fn get_summary(&self) -> &str {
        &self.summary
    }

    pub fn get_description(&self) -> &str {
        &self.description
    }
}

/// A settings schema (`GSettingsSchema`).
#[derive(Clone, Debug)]
pub struct SettingsSchema {
    id: String,
    path: Option<String>,
    keys: BTreeMap<String, SettingsSchemaKey>,
}

impl SettingsSchema {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            path: None,
            keys: BTreeMap::new(),
        }
    }

    pub fn with_path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }

    pub fn add_key(&mut self, key: SettingsSchemaKey) {
        self.keys.insert(key.name.clone(), key);
    }

    pub fn get_id(&self) -> &str {
        &self.id
    }

    pub fn get_path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    pub fn has_key(&self, name: &str) -> bool {
        self.keys.contains_key(name)
    }

    pub fn get_key(&self, name: &str) -> Option<&SettingsSchemaKey> {
        self.keys.get(name)
    }

    pub fn list_keys(&self) -> Vec<String> {
        self.keys.keys().cloned().collect()
    }

    pub fn list_children(&self) -> Vec<String> {
        Vec::new()
    }
}

/// A schema source (`GSettingsSchemaSource`).
pub struct SettingsSchemaSource {
    schemas: Mutex<BTreeMap<String, SettingsSchema>>,
    parent: Option<&'static SettingsSchemaSource>,
}

impl SettingsSchemaSource {
    /// Creates a new schema source.
    pub fn new() -> Self {
        Self {
            schemas: Mutex::new(BTreeMap::new()),
            parent: None,
        }
    }

    /// Creates a new schema source with a parent.
    pub fn with_parent(parent: &'static SettingsSchemaSource) -> Self {
        Self {
            schemas: Mutex::new(BTreeMap::new()),
            parent: Some(parent),
        }
    }

    /// Adds a schema to the source.
    pub fn add_schema(&self, schema: SettingsSchema) {
        self.schemas.lock().insert(schema.id.clone(), schema);
    }

    /// Looks up a schema by ID.
    ///
    /// Mirrors `g_settings_schema_source_lookup`.
    pub fn lookup(&self, schema_id: &str, recursive: bool) -> Option<SettingsSchema> {
        if let Some(schema) = self.schemas.lock().get(schema_id) {
            return Some(schema.clone());
        }
        if recursive {
            if let Some(parent) = self.parent {
                return parent.lookup(schema_id, true);
            }
        }
        None
    }

    /// Lists all schemas in this source.
    pub fn list_schemas(&self) -> Vec<String> {
        self.schemas.lock().keys().cloned().collect()
    }
}

impl Default for SettingsSchemaSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the default schema source.
static DEFAULT_SOURCE: Mutex<Option<SettingsSchemaSource>> = Mutex::new(None);

pub fn get_default_source() -> SettingsSchemaSource {
    let mut guard = DEFAULT_SOURCE.lock();
    if guard.is_none() {
        let source = SettingsSchemaSource::new();
        *guard = Some(source);
    }
    guard.as_ref().unwrap().clone()
}

impl Clone for SettingsSchemaSource {
    fn clone(&self) -> Self {
        Self {
            schemas: Mutex::new(self.schemas.lock().clone()),
            parent: self.parent,
        }
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_schema() -> SettingsSchema {
        let mut schema = SettingsSchema::new("org.test.App").with_path("/org/test/app/");
        schema.add_key(SettingsSchemaKey::new("window-width", "i", "800"));
        schema.add_key(SettingsSchemaKey::new("window-height", "i", "600"));
        schema.add_key(SettingsSchemaKey::new("dark-mode", "b", "false"));
        schema
    }

    #[test]
    fn test_schema_new() {
        let schema = SettingsSchema::new("org.test.App");
        assert_eq!(schema.get_id(), "org.test.App");
        assert_eq!(schema.get_path(), None);
        assert!(!schema.has_key("foo"));
    }

    #[test]
    fn test_schema_with_path() {
        let schema = SettingsSchema::new("org.test.App").with_path("/org/test/app/");
        assert_eq!(schema.get_path(), Some("/org/test/app/"));
    }

    #[test]
    fn test_schema_add_and_has_key() {
        let schema = make_test_schema();
        assert!(schema.has_key("window-width"));
        assert!(schema.has_key("dark-mode"));
        assert!(!schema.has_key("nonexistent"));
    }

    #[test]
    fn test_schema_get_key() {
        let schema = make_test_schema();
        let key = schema.get_key("window-width").unwrap();
        assert_eq!(key.get_name(), "window-width");
        assert_eq!(key.get_value_type(), "i");
        assert_eq!(key.get_default_value(), "800");
    }

    #[test]
    fn test_schema_list_keys() {
        let schema = make_test_schema();
        let keys = schema.list_keys();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"window-width".to_string()));
        assert!(keys.contains(&"dark-mode".to_string()));
    }

    #[test]
    fn test_schema_source_new() {
        let source = SettingsSchemaSource::new();
        assert!(source.list_schemas().is_empty());
    }

    #[test]
    fn test_schema_source_add_and_lookup() {
        let source = SettingsSchemaSource::new();
        source.add_schema(make_test_schema());
        let schema = source.lookup("org.test.App", false).unwrap();
        assert_eq!(schema.get_id(), "org.test.App");
        assert!(schema.has_key("window-width"));
    }

    #[test]
    fn test_schema_source_lookup_not_found() {
        let source = SettingsSchemaSource::new();
        assert!(source.lookup("nonexistent", false).is_none());
    }

    #[test]
    fn test_schema_source_list_schemas() {
        let source = SettingsSchemaSource::new();
        source.add_schema(SettingsSchema::new("org.test.A"));
        source.add_schema(SettingsSchema::new("org.test.B"));
        let schemas = source.list_schemas();
        assert_eq!(schemas.len(), 2);
        assert!(schemas.contains(&"org.test.A".to_string()));
    }
}
