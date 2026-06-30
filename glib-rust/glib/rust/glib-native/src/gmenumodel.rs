//! GMenuModel matching `gio/gmenumodel.h`.
//!
//! Upstream `GMenuModel` is an abstract interface for menu models.
//! We port it as a Rust trait with a simple in-memory implementation.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Menu attribute name constants.
pub const MENU_ATTRIBUTE_ACTION: &str = "action";
pub const MENU_ATTRIBUTE_ACTION_NAMESPACE: &str = "action-namespace";
pub const MENU_ATTRIBUTE_TARGET: &str = "target";
pub const MENU_ATTRIBUTE_LABEL: &str = "label";
pub const MENU_ATTRIBUTE_ICON: &str = "icon";

/// Menu link name constants.
pub const MENU_LINK_SUBMENU: &str = "submenu";
pub const MENU_LINK_SECTION: &str = "section";

/// Trait for menu models (`GMenuModel`).
pub trait MenuModel {
    /// Checks if the model is mutable (can change).
    fn is_mutable(&self) -> bool;

    /// Gets the number of items.
    fn get_n_items(&self) -> usize;

    /// Gets an attribute value for an item.
    fn get_item_attribute_value(&self, item_index: usize, attribute: &str) -> Option<String>;

    /// Gets all attributes for an item.
    fn get_item_attributes(&self, item_index: usize) -> BTreeMap<String, String>;

    /// Gets a linked menu model for an item.
    fn get_item_link(&self, item_index: usize, link: &str) -> Option<SimpleMenuModel>;
}

/// A simple in-memory menu model implementation.
pub struct SimpleMenuModel {
    items: Mutex<Vec<MenuItemData>>,
    mutable: Mutex<bool>,
}

#[derive(Clone)]
struct MenuItemData {
    attributes: BTreeMap<String, String>,
    links: BTreeMap<String, SimpleMenuModel>,
}

impl Clone for SimpleMenuModel {
    fn clone(&self) -> Self {
        Self {
            items: Mutex::new(self.items.lock().clone()),
            mutable: Mutex::new(*self.mutable.lock()),
        }
    }
}

impl SimpleMenuModel {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(Vec::new()),
            mutable: Mutex::new(true),
        }
    }

    pub fn append(&self, attributes: BTreeMap<String, String>) {
        if !*self.mutable.lock() {
            return;
        }
        self.items.lock().push(MenuItemData {
            attributes,
            links: BTreeMap::new(),
        });
    }

    pub fn append_with_links(
        &self,
        attributes: BTreeMap<String, String>,
        links: BTreeMap<String, SimpleMenuModel>,
    ) {
        if !*self.mutable.lock() {
            return;
        }
        self.items.lock().push(MenuItemData { attributes, links });
    }

    pub fn freeze(&self) {
        *self.mutable.lock() = false;
    }
}

impl Default for SimpleMenuModel {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuModel for SimpleMenuModel {
    fn is_mutable(&self) -> bool {
        *self.mutable.lock()
    }

    fn get_n_items(&self) -> usize {
        self.items.lock().len()
    }

    fn get_item_attribute_value(&self, item_index: usize, attribute: &str) -> Option<String> {
        let items = self.items.lock();
        items
            .get(item_index)
            .and_then(|item| item.attributes.get(attribute).cloned())
    }

    fn get_item_attributes(&self, item_index: usize) -> BTreeMap<String, String> {
        let items = self.items.lock();
        items
            .get(item_index)
            .map(|item| item.attributes.clone())
            .unwrap_or_default()
    }

    fn get_item_link(&self, item_index: usize, link: &str) -> Option<SimpleMenuModel> {
        let items = self.items.lock();
        let item = items.get(item_index)?;
        item.links.get(link).cloned()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_attrs(label: &str, action: &str) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert(MENU_ATTRIBUTE_LABEL.to_string(), label.to_string());
        m.insert(MENU_ATTRIBUTE_ACTION.to_string(), action.to_string());
        m
    }

    #[test]
    fn test_new() {
        let model = SimpleMenuModel::new();
        assert!(model.is_mutable());
        assert_eq!(model.get_n_items(), 0);
    }

    #[test]
    fn test_append_and_get_n_items() {
        let model = SimpleMenuModel::new();
        model.append(make_attrs("Open", "app.open"));
        model.append(make_attrs("Save", "app.save"));
        assert_eq!(model.get_n_items(), 2);
    }

    #[test]
    fn test_get_item_attribute_value() {
        let model = SimpleMenuModel::new();
        model.append(make_attrs("Open", "app.open"));
        assert_eq!(
            model.get_item_attribute_value(0, MENU_ATTRIBUTE_LABEL),
            Some("Open".to_string())
        );
        assert_eq!(
            model.get_item_attribute_value(0, MENU_ATTRIBUTE_ACTION),
            Some("app.open".to_string())
        );
        assert_eq!(model.get_item_attribute_value(0, "nonexistent"), None);
    }

    #[test]
    fn test_get_item_attributes() {
        let model = SimpleMenuModel::new();
        model.append(make_attrs("Quit", "app.quit"));
        let attrs = model.get_item_attributes(0);
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs.get(MENU_ATTRIBUTE_LABEL), Some(&"Quit".to_string()));
    }

    #[test]
    fn test_freeze() {
        let model = SimpleMenuModel::new();
        model.append(make_attrs("A", "app.a"));
        model.freeze();
        assert!(!model.is_mutable());
        model.append(make_attrs("B", "app.b"));
        assert_eq!(model.get_n_items(), 1);
    }

    #[test]
    fn test_get_item_attributes_out_of_range() {
        let model = SimpleMenuModel::new();
        let attrs = model.get_item_attributes(0);
        assert!(attrs.is_empty());
    }

    #[test]
    fn test_attribute_constants() {
        assert_eq!(MENU_ATTRIBUTE_ACTION, "action");
        assert_eq!(MENU_ATTRIBUTE_LABEL, "label");
        assert_eq!(MENU_ATTRIBUTE_TARGET, "target");
        assert_eq!(MENU_LINK_SUBMENU, "submenu");
        assert_eq!(MENU_LINK_SECTION, "section");
    }
}
