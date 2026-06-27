//! GDBusMenuModel matching `gio/gdbusmenumodel.h`.
//!
//! A D-Bus-based menu model. In this no_std port we model a simple
//! menu with items stored in memory.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A menu item in a D-Bus menu model.
#[derive(Debug, Clone)]
pub struct DBusMenuItem {
    pub label: String,
    pub action: Option<String>,
    pub submenu: Option<String>,
}

impl DBusMenuItem {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            action: None,
            submenu: None,
        }
    }

    pub fn with_action(label: &str, action: &str) -> Self {
        Self {
            label: label.to_string(),
            action: Some(action.to_string()),
            submenu: None,
        }
    }

    pub fn with_submenu(label: &str, submenu: &str) -> Self {
        Self {
            label: label.to_string(),
            action: None,
            submenu: Some(submenu.to_string()),
        }
    }
}

/// A D-Bus menu model (`GDBusMenuModel`).
pub struct DBusMenuModel {
    bus_name: Mutex<String>,
    object_path: Mutex<String>,
    items: Mutex<Vec<DBusMenuItem>>,
}

impl DBusMenuModel {
    /// Creates a new D-Bus menu model.
    ///
    /// Mirrors `g_dbus_menu_model_get`.
    pub fn new(bus_name: &str, object_path: &str) -> Self {
        Self {
            bus_name: Mutex::new(bus_name.to_string()),
            object_path: Mutex::new(object_path.to_string()),
            items: Mutex::new(Vec::new()),
        }
    }

    /// Gets the bus name.
    pub fn get_bus_name(&self) -> String {
        self.bus_name.lock().clone()
    }

    /// Gets the object path.
    pub fn get_object_path(&self) -> String {
        self.object_path.lock().clone()
    }

    /// Adds a menu item.
    pub fn add_item(&self, item: DBusMenuItem) {
        self.items.lock().push(item);
    }

    /// Gets the number of items.
    pub fn get_n_items(&self) -> usize {
        self.items.lock().len()
    }

    /// Gets an item by index.
    pub fn get_item(&self, index: usize) -> Option<DBusMenuItem> {
        self.items.lock().get(index).cloned()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let menu = DBusMenuModel::new("org.test.Menu", "/org/test/menu");
        assert_eq!(menu.get_bus_name(), "org.test.Menu");
        assert_eq!(menu.get_object_path(), "/org/test/menu");
        assert_eq!(menu.get_n_items(), 0);
    }

    #[test]
    fn test_add_items() {
        let menu = DBusMenuModel::new("org.test", "/menu");
        menu.add_item(DBusMenuItem::new("File"));
        menu.add_item(DBusMenuItem::with_action("Open", "app.open"));
        menu.add_item(DBusMenuItem::with_submenu("Recent", "/menu/recent"));
        assert_eq!(menu.get_n_items(), 3);
    }

    #[test]
    fn test_get_item() {
        let menu = DBusMenuModel::new("org.test", "/menu");
        menu.add_item(DBusMenuItem::with_action("Save", "app.save"));
        let item = menu.get_item(0).unwrap();
        assert_eq!(item.label, "Save");
        assert_eq!(item.action, Some("app.save".to_string()));
    }

    #[test]
    fn test_get_item_out_of_bounds() {
        let menu = DBusMenuModel::new("org.test", "/menu");
        assert!(menu.get_item(0).is_none());
    }
}
