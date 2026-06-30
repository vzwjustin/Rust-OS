//! GListModel interface matching `gio/glistmodel.h`.
//!
//! Upstream `GListModel` is a `GInterface` for list-based models.
//! We port it as a Rust trait.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;

/// Item type identifier for list model items.
///
/// In upstream, this is a `GType`. We use a string-based type identifier
/// for simplicity in `no_std`.
pub type ItemType = String;

/// Trait for list-based models (`GListModel`).
pub trait ListModel {
    /// Gets the type of items in the list.
    ///
    /// Mirrors `g_list_model_get_item_type`.
    fn get_item_type(&self) -> ItemType;

    /// Gets the number of items in the list.
    ///
    /// Mirrors `g_list_model_get_n_items`.
    fn get_n_items(&self) -> usize;

    /// Gets the item at `position`.
    ///
    /// Returns `None` if `position` is out of bounds.
    ///
    /// Mirrors `g_list_model_get_item`.
    fn get_item(&self, position: usize) -> Option<String>;

    /// Emits the "items-changed" signal.
    ///
    /// Mirrors `g_list_model_items_changed`.
    /// In our no_std port, this is a no-op callback hook.
    fn items_changed(&self, _position: usize, _removed: usize, _added: usize) {}
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct TestListModel {
        items: Vec<String>,
    }

    impl TestListModel {
        fn new(items: Vec<&str>) -> Self {
            Self {
                items: items.iter().map(|s| s.to_string()).collect(),
            }
        }
    }

    impl ListModel for TestListModel {
        fn get_item_type(&self) -> ItemType {
            "s".to_string()
        }

        fn get_n_items(&self) -> usize {
            self.items.len()
        }

        fn get_item(&self, position: usize) -> Option<String> {
            self.items.get(position).cloned()
        }
    }

    #[test]
    fn test_list_model_n_items() {
        let model = TestListModel::new(vec!["a", "b", "c"]);
        assert_eq!(model.get_n_items(), 3);
    }

    #[test]
    fn test_list_model_get_item() {
        let model = TestListModel::new(vec!["a", "b", "c"]);
        assert_eq!(model.get_item(0).unwrap(), "a");
        assert_eq!(model.get_item(1).unwrap(), "b");
        assert_eq!(model.get_item(2).unwrap(), "c");
    }

    #[test]
    fn test_list_model_get_item_out_of_bounds() {
        let model = TestListModel::new(vec!["a"]);
        assert!(model.get_item(5).is_none());
    }

    #[test]
    fn test_list_model_empty() {
        let model = TestListModel::new(vec![]);
        assert_eq!(model.get_n_items(), 0);
        assert!(model.get_item(0).is_none());
    }

    #[test]
    fn test_list_model_item_type() {
        let model = TestListModel::new(vec!["a"]);
        assert_eq!(model.get_item_type(), "s");
    }
}
