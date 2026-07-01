//! Session state base type ported from GNOME Mutter's src/core/meta-session-state.c
//!
//! Abstract interface for session state serialization, persistence, and restoration.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-session-state.c

use alloc::collections::BTreeMap;
use alloc::string::String;

/// Abstract session state for window/compositor state persistence
#[derive(Debug, Clone)]
pub struct SessionState {
    pub name: String,
    pub data: BTreeMap<String, String>,
}

impl SessionState {
    /// Create new session state
    pub fn new(name: String) -> Self {
        SessionState {
            name,
            data: BTreeMap::new(),
        }
    }

    /// Get session state name
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Serialize state to key-value map
    /// Stub: abstract method; implementations handle specific serialization formats
    pub fn serialize(&self) -> BTreeMap<String, String> {
        self.data.clone()
    }

    /// Parse state from key-value map
    /// Stub: abstract method; implementations handle specific parsing formats
    pub fn parse(&mut self, _data: &BTreeMap<String, String>) -> bool {
        // Derived classes would implement specific parsing logic
        true
    }

    /// Save window state for later restoration
    /// Stub: implementations serialize window properties to persistent storage
    pub fn save_window(&mut self, _window_name: &str) {
        // Would serialize window geometry, properties, etc.
    }

    /// Restore window state from previously saved data
    /// Stub: implementations restore window properties from persistent storage
    pub fn restore_window(&self, _window_name: &str) -> bool {
        // Would restore window geometry, properties, etc.
        true
    }

    /// Remove stored window state
    pub fn remove_window(&mut self, _window_name: &str) {
        // Would delete window state from storage
    }

    /// Store a property in the state
    pub fn set_property(&mut self, key: String, value: String) {
        self.data.insert(key, value);
    }

    /// Retrieve a property from the state
    pub fn get_property(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }

    /// Clear all state
    pub fn clear(&mut self) {
        self.data.clear();
    }
}
