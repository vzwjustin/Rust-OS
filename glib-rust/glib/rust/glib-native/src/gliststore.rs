//! GListStore matching `gio/gliststore.h`.
//!
//! Upstream `GListStore` is a `GObject` implementing `GListModel` that
//! stores items in a simple list. We port it as a struct with
//! `Mutex`-protected `Vec<String>` storage.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::glistmodel::{ItemType, ListModel};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A simple list store (`GListStore`).
///
/// Implements `ListModel` with `Vec<String>` storage.
pub struct ListStore {
    item_type: ItemType,
    items: Mutex<Vec<String>>,
}

impl ListStore {
    /// Creates a new list store for the given item type.
    ///
    /// Mirrors `g_list_store_new`.
    pub fn new(item_type: &str) -> Self {
        Self {
            item_type: item_type.to_string(),
            items: Mutex::new(Vec::new()),
        }
    }

    /// Appends an item to the end of the store.
    ///
    /// Mirrors `g_list_store_append`.
    pub fn append(&self, item: &str) {
        self.items.lock().push(item.to_string());
    }

    /// Inserts an item at `position`.
    ///
    /// Mirrors `g_list_store_insert`.
    pub fn insert(&self, position: usize, item: &str) {
        let mut items = self.items.lock();
        if position <= items.len() {
            items.insert(position, item.to_string());
        }
    }

    /// Removes the item at `position`.
    ///
    /// Mirrors `g_list_store_remove`.
    pub fn remove(&self, position: usize) {
        let mut items = self.items.lock();
        if position < items.len() {
            items.remove(position);
        }
    }

    /// Removes all items.
    ///
    /// Mirrors `g_list_store_remove_all`.
    pub fn remove_all(&self) {
        self.items.lock().clear();
    }

    /// Finds the position of an item.
    ///
    /// Mirrors `g_list_store_find`.
    /// Returns `Some(position)` if found, `None` otherwise.
    pub fn find(&self, item: &str) -> Option<usize> {
        self.items.lock().iter().position(|s| s == item)
    }

    /// Splices items: removes `n_removals` at `position`, then inserts `additions`.
    ///
    /// Mirrors `g_list_store_splice`.
    pub fn splice(&self, position: usize, n_removals: usize, additions: &[&str]) {
        let mut items = self.items.lock();
        let pos = position.min(items.len());
        let remove_count = n_removals.min(items.len().saturating_sub(pos));
        for _ in 0..remove_count {
            items.remove(pos);
        }
        for (i, item) in additions.iter().enumerate() {
            items.insert(pos + i, item.to_string());
        }
    }

    /// Sorts the store using a comparison function.
    ///
    /// Mirrors `g_list_store_sort`.
    pub fn sort<F>(&self, compare: F)
    where
        F: Fn(&str, &str) -> core::cmp::Ordering,
    {
        self.items.lock().sort_by(|a, b| compare(a, b));
    }

    /// Gets the number of items.
    pub fn n_items(&self) -> usize {
        self.items.lock().len()
    }
}

impl ListModel for ListStore {
    fn get_item_type(&self) -> ItemType {
        self.item_type.clone()
    }

    fn get_n_items(&self) -> usize {
        self.n_items()
    }

    fn get_item(&self, position: usize) -> Option<String> {
        self.items.lock().get(position).cloned()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_store_new() {
        let store = ListStore::new("s");
        assert_eq!(store.get_item_type(), "s");
        assert_eq!(store.get_n_items(), 0);
    }

    #[test]
    fn test_list_store_append() {
        let store = ListStore::new("s");
        store.append("a");
        store.append("b");
        store.append("c");
        assert_eq!(store.get_n_items(), 3);
        assert_eq!(store.get_item(0).unwrap(), "a");
        assert_eq!(store.get_item(1).unwrap(), "b");
        assert_eq!(store.get_item(2).unwrap(), "c");
    }

    #[test]
    fn test_list_store_insert() {
        let store = ListStore::new("s");
        store.append("a");
        store.append("c");
        store.insert(1, "b");
        assert_eq!(store.get_item(0).unwrap(), "a");
        assert_eq!(store.get_item(1).unwrap(), "b");
        assert_eq!(store.get_item(2).unwrap(), "c");
    }

    #[test]
    fn test_list_store_remove() {
        let store = ListStore::new("s");
        store.append("a");
        store.append("b");
        store.append("c");
        store.remove(1);
        assert_eq!(store.get_n_items(), 2);
        assert_eq!(store.get_item(0).unwrap(), "a");
        assert_eq!(store.get_item(1).unwrap(), "c");
    }

    #[test]
    fn test_list_store_remove_all() {
        let store = ListStore::new("s");
        store.append("a");
        store.append("b");
        store.remove_all();
        assert_eq!(store.get_n_items(), 0);
    }

    #[test]
    fn test_list_store_find() {
        let store = ListStore::new("s");
        store.append("a");
        store.append("b");
        store.append("c");
        assert_eq!(store.find("b"), Some(1));
        assert_eq!(store.find("d"), None);
    }

    #[test]
    fn test_list_store_splice() {
        let store = ListStore::new("s");
        store.append("a");
        store.append("b");
        store.append("c");
        store.splice(1, 1, &["x", "y"]);
        assert_eq!(store.get_n_items(), 4);
        assert_eq!(store.get_item(0).unwrap(), "a");
        assert_eq!(store.get_item(1).unwrap(), "x");
        assert_eq!(store.get_item(2).unwrap(), "y");
        assert_eq!(store.get_item(3).unwrap(), "c");
    }

    #[test]
    fn test_list_store_sort() {
        let store = ListStore::new("s");
        store.append("c");
        store.append("a");
        store.append("b");
        store.sort(|a, b| a.cmp(b));
        assert_eq!(store.get_item(0).unwrap(), "a");
        assert_eq!(store.get_item(1).unwrap(), "b");
        assert_eq!(store.get_item(2).unwrap(), "c");
    }

    #[test]
    fn test_list_store_as_list_model() {
        let store = ListStore::new("s");
        store.append("hello");
        let model: &dyn ListModel = &store;
        assert_eq!(model.get_n_items(), 1);
        assert_eq!(model.get_item(0).unwrap(), "hello");
    }
}
