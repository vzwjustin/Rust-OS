//! GMenu and GMenuItem matching `gio/gmenu.h`.
//!
//! Upstream `GMenu` is a simple implementation of `GMenuModel` for
//! building menus programmatically. We port `GMenu` and `GMenuItem`
//! as plain Rust structs.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A menu item (`GMenuItem`).
#[derive(Clone)]
pub struct MenuItem {
    label: Option<String>,
    action: Option<String>,
    target: Option<String>,
    icon: Option<String>,
    section: Option<Vec<MenuItem>>,
    submenu: Option<Vec<MenuItem>>,
    attributes: Vec<(String, String)>,
}

impl MenuItem {
    /// Creates a new menu item with a label and detailed action.
    ///
    /// Mirrors `g_menu_item_new`.
    pub fn new(label: Option<&str>, detailed_action: Option<&str>) -> Self {
        Self {
            label: label.map(|s| s.to_string()),
            action: detailed_action.map(|s| s.to_string()),
            target: None,
            icon: None,
            section: None,
            submenu: None,
            attributes: Vec::new(),
        }
    }

    /// Creates a new submenu menu item.
    ///
    /// Mirrors `g_menu_item_new_submenu`.
    pub fn new_submenu(label: &str, submenu: Vec<MenuItem>) -> Self {
        Self {
            label: Some(label.to_string()),
            action: None,
            target: None,
            icon: None,
            section: None,
            submenu: Some(submenu),
            attributes: Vec::new(),
        }
    }

    /// Creates a new section menu item.
    ///
    /// Mirrors `g_menu_item_new_section`.
    pub fn new_section(label: &str, section: Vec<MenuItem>) -> Self {
        Self {
            label: Some(label.to_string()),
            action: None,
            target: None,
            icon: None,
            section: Some(section),
            submenu: None,
            attributes: Vec::new(),
        }
    }

    pub fn get_label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn set_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    pub fn get_action(&self) -> Option<&str> {
        self.action.as_deref()
    }

    pub fn set_detailed_action(&mut self, detailed_action: &str) {
        self.action = Some(detailed_action.to_string());
    }

    pub fn set_action_and_target(&mut self, action: &str, target: Option<&str>) {
        self.action = Some(action.to_string());
        self.target = target.map(|s| s.to_string());
    }

    pub fn get_target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    pub fn set_icon(&mut self, icon: &str) {
        self.icon = Some(icon.to_string());
    }

    pub fn get_icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    pub fn set_submenu(&mut self, submenu: Vec<MenuItem>) {
        self.submenu = Some(submenu);
    }

    pub fn get_submenu(&self) -> Option<&Vec<MenuItem>> {
        self.submenu.as_ref()
    }

    pub fn set_section(&mut self, section: Vec<MenuItem>) {
        self.section = Some(section);
    }

    pub fn get_section(&self) -> Option<&Vec<MenuItem>> {
        self.section.as_ref()
    }

    pub fn set_attribute(&mut self, attribute: &str, value: &str) {
        self.attributes
            .push((attribute.to_string(), value.to_string()));
    }

    pub fn get_attribute(&self, attribute: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == attribute)
            .map(|(_, v)| v.as_str())
    }
}

/// A menu (`GMenu`).
pub struct Menu {
    items: Mutex<Vec<MenuItem>>,
    frozen: Mutex<bool>,
}

impl Menu {
    /// Creates a new empty menu.
    ///
    /// Mirrors `g_menu_new`.
    pub fn new() -> Self {
        Self {
            items: Mutex::new(Vec::new()),
            frozen: Mutex::new(false),
        }
    }

    /// Freezes the menu, preventing further modifications.
    ///
    /// Mirrors `g_menu_freeze`.
    pub fn freeze(&self) {
        *self.frozen.lock() = true;
    }

    pub fn is_frozen(&self) -> bool {
        *self.frozen.lock()
    }

    /// Appends an item to the menu.
    ///
    /// Mirrors `g_menu_append_item`.
    pub fn append_item(&self, item: MenuItem) {
        if *self.frozen.lock() {
            return;
        }
        self.items.lock().push(item);
    }

    /// Prepends an item to the menu.
    ///
    /// Mirrors `g_menu_prepend_item`.
    pub fn prepend_item(&self, item: MenuItem) {
        if *self.frozen.lock() {
            return;
        }
        self.items.lock().insert(0, item);
    }

    /// Inserts an item at the given position.
    ///
    /// Mirrors `g_menu_insert_item`.
    pub fn insert_item(&self, position: usize, item: MenuItem) {
        if *self.frozen.lock() {
            return;
        }
        let mut items = self.items.lock();
        let pos = position.min(items.len());
        items.insert(pos, item);
    }

    /// Appends a simple label+action item.
    ///
    /// Mirrors `g_menu_append`.
    pub fn append(&self, label: &str, detailed_action: &str) {
        self.append_item(MenuItem::new(Some(label), Some(detailed_action)));
    }

    /// Prepends a simple label+action item.
    ///
    /// Mirrors `g_menu_prepend`.
    pub fn prepend(&self, label: &str, detailed_action: &str) {
        self.prepend_item(MenuItem::new(Some(label), Some(detailed_action)));
    }

    /// Removes the item at the given position.
    ///
    /// Mirrors `g_menu_remove`.
    pub fn remove(&self, position: usize) {
        if *self.frozen.lock() {
            return;
        }
        let mut items = self.items.lock();
        if position < items.len() {
            items.remove(position);
        }
    }

    /// Removes all items.
    ///
    /// Mirrors `g_menu_remove_all`.
    pub fn remove_all(&self) {
        if *self.frozen.lock() {
            return;
        }
        self.items.lock().clear();
    }

    /// Gets the number of items.
    pub fn get_n_items(&self) -> usize {
        self.items.lock().len()
    }

    /// Gets a snapshot of all items.
    pub fn get_items(&self) -> Vec<MenuItem> {
        self.items.lock().clone()
    }
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_new() {
        let menu = Menu::new();
        assert_eq!(menu.get_n_items(), 0);
        assert!(!menu.is_frozen());
    }

    #[test]
    fn test_append() {
        let menu = Menu::new();
        menu.append("Open", "app.open");
        menu.append("Save", "app.save");
        assert_eq!(menu.get_n_items(), 2);
        let items = menu.get_items();
        assert_eq!(items[0].get_label(), Some("Open"));
        assert_eq!(items[0].get_action(), Some("app.open"));
    }

    #[test]
    fn test_prepend() {
        let menu = Menu::new();
        menu.append("Second", "app.second");
        menu.prepend("First", "app.first");
        assert_eq!(menu.get_n_items(), 2);
        let items = menu.get_items();
        assert_eq!(items[0].get_label(), Some("First"));
    }

    #[test]
    fn test_insert() {
        let menu = Menu::new();
        menu.append("A", "app.a");
        menu.append("C", "app.c");
        menu.insert_item(1, MenuItem::new(Some("B"), Some("app.b")));
        assert_eq!(menu.get_n_items(), 3);
        let items = menu.get_items();
        assert_eq!(items[1].get_label(), Some("B"));
    }

    #[test]
    fn test_remove() {
        let menu = Menu::new();
        menu.append("A", "app.a");
        menu.append("B", "app.b");
        menu.remove(0);
        assert_eq!(menu.get_n_items(), 1);
        let items = menu.get_items();
        assert_eq!(items[0].get_label(), Some("B"));
    }

    #[test]
    fn test_remove_all() {
        let menu = Menu::new();
        menu.append("A", "app.a");
        menu.append("B", "app.b");
        menu.remove_all();
        assert_eq!(menu.get_n_items(), 0);
    }

    #[test]
    fn test_freeze() {
        let menu = Menu::new();
        menu.append("A", "app.a");
        menu.freeze();
        assert!(menu.is_frozen());
        menu.append("B", "app.b");
        assert_eq!(menu.get_n_items(), 1);
    }

    #[test]
    fn test_menu_item_new() {
        let item = MenuItem::new(Some("Quit"), Some("app.quit"));
        assert_eq!(item.get_label(), Some("Quit"));
        assert_eq!(item.get_action(), Some("app.quit"));
    }

    #[test]
    fn test_menu_item_submenu() {
        let submenu_items = vec![MenuItem::new(Some("Copy"), Some("app.copy"))];
        let item = MenuItem::new_submenu("Edit", submenu_items);
        assert_eq!(item.get_label(), Some("Edit"));
        assert!(item.get_submenu().is_some());
        assert_eq!(item.get_submenu().unwrap().len(), 1);
    }

    #[test]
    fn test_menu_item_section() {
        let section_items = vec![MenuItem::new(Some("Cut"), Some("app.cut"))];
        let item = MenuItem::new_section("Actions", section_items);
        assert_eq!(item.get_label(), Some("Actions"));
        assert!(item.get_section().is_some());
        assert_eq!(item.get_section().unwrap().len(), 1);
    }

    #[test]
    fn test_menu_item_attributes() {
        let mut item = MenuItem::new(Some("Test"), Some("app.test"));
        item.set_attribute("custom", "value123");
        assert_eq!(item.get_attribute("custom"), Some("value123"));
        assert_eq!(item.get_attribute("nonexistent"), None);
    }

    #[test]
    fn test_menu_item_icon() {
        let mut item = MenuItem::new(Some("Test"), Some("app.test"));
        item.set_icon("document-open");
        assert_eq!(item.get_icon(), Some("document-open"));
    }

    #[test]
    fn test_menu_item_action_and_target() {
        let mut item = MenuItem::new(Some("Test"), None);
        item.set_action_and_target("app.open", Some("/path/to/file"));
        assert_eq!(item.get_action(), Some("app.open"));
        assert_eq!(item.get_target(), Some("/path/to/file"));
    }
}
