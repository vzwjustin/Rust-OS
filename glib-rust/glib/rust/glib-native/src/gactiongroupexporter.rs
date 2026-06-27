//! GActionGroupExporter matching `gio/gactiongroupexporter.h`.
//!
//! Exports a `GActionGroup` on D-Bus. In this no_std port we model
//! the export state with a registry of exported groups.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// An action group exporter (`GActionGroupExporter`).
pub struct ActionGroupExporter {
    exports: Mutex<BTreeMap<String, Vec<String>>>,
}

impl ActionGroupExporter {
    /// Creates a new exporter.
    pub fn new() -> Self {
        Self {
            exports: Mutex::new(BTreeMap::new()),
        }
    }

    /// Exports an action group on a D-Bus connection.
    ///
    /// Mirrors `g_dbus_connection_export_action_group`.
    pub fn export(&self, object_path: &str, actions: Vec<String>) -> bool {
        let mut exports = self.exports.lock();
        if exports.contains_key(object_path) {
            return false;
        }
        exports.insert(object_path.to_string(), actions);
        true
    }

    /// Unexports an action group.
    ///
    /// Mirrors `g_dbus_connection_unexport_action_group`.
    pub fn unexport(&self, object_path: &str) -> bool {
        self.exports.lock().remove(object_path).is_some()
    }

    /// Gets the actions for an exported path.
    pub fn get_exported_actions(&self, object_path: &str) -> Option<Vec<String>> {
        self.exports.lock().get(object_path).cloned()
    }

    /// Returns the number of exported groups.
    pub fn export_count(&self) -> usize {
        self.exports.lock().len()
    }
}

impl Default for ActionGroupExporter {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let e = ActionGroupExporter::new();
        assert_eq!(e.export_count(), 0);
    }

    #[test]
    fn test_export_unexport() {
        let e = ActionGroupExporter::new();
        assert!(e.export("/actions", vec!["save".to_string(), "open".to_string()]));
        assert_eq!(e.export_count(), 1);
        assert!(e.unexport("/actions"));
        assert_eq!(e.export_count(), 0);
    }

    #[test]
    fn test_duplicate_export_fails() {
        let e = ActionGroupExporter::new();
        e.export("/actions", vec![]);
        assert!(!e.export("/actions", vec![]));
    }

    #[test]
    fn test_get_exported_actions() {
        let e = ActionGroupExporter::new();
        e.export("/actions", vec!["quit".to_string()]);
        let actions = e.get_exported_actions("/actions").unwrap();
        assert_eq!(actions, vec!["quit".to_string()]);
    }
}
