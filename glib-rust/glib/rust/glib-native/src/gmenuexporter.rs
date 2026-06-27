//! GMenuExporter matching `gio/gmenuexporter.h`.
//! Exports a `GMenuModel` on D-Bus. In this no_std port we model
//! the export registry with path-to-menu mapping.
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A menu exporter (`GMenuExporter`).
pub struct MenuExporter {
    exports: Mutex<BTreeMap<String, Vec<String>>>,
}

impl MenuExporter {
    pub fn new() -> Self {
        Self {
            exports: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn export(&self, object_path: &str, items: Vec<String>) -> bool {
        let mut exports = self.exports.lock();
        if exports.contains_key(object_path) {
            return false;
        }
        exports.insert(object_path.to_string(), items);
        true
    }

    pub fn unexport(&self, object_path: &str) -> bool {
        self.exports.lock().remove(object_path).is_some()
    }

    pub fn get_exported(&self, object_path: &str) -> Option<Vec<String>> {
        self.exports.lock().get(object_path).cloned()
    }

    pub fn export_count(&self) -> usize {
        self.exports.lock().len()
    }
}

impl Default for MenuExporter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_unexport() {
        let e = MenuExporter::new();
        assert!(e.export("/menu", vec!["File".to_string(), "Edit".to_string()]));
        assert_eq!(e.export_count(), 1);
        assert!(e.unexport("/menu"));
        assert_eq!(e.export_count(), 0);
    }

    #[test]
    fn test_duplicate_fails() {
        let e = MenuExporter::new();
        e.export("/menu", vec![]);
        assert!(!e.export("/menu", vec![]));
    }
}
