//! String vector builder matching `gstrvbuilder.h` / `gstrvbuilder.c`.
//!
//! A simple builder for constructing null-terminated string arrays.
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;

/// A string vector builder (`GStrvBuilder`).
///
/// Accumulates strings and produces a `Vec<String>` (equivalent to `GStrv`).
pub struct StrvBuilder {
    items: Vec<String>,
}

impl StrvBuilder {
    /// Create a new builder (`g_strv_builder_new`).
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Add a string (`g_strv_builder_add`).
    pub fn add(&mut self, value: &str) {
        self.items.push(value.to_owned());
    }

    /// Add multiple strings (`g_strv_builder_addv`).
    pub fn addv(&mut self, values: &[&str]) {
        for v in values {
            self.items.push((*v).to_owned());
        }
    }

    /// Take a string, avoiding a clone (`g_strv_builder_take`).
    pub fn take(&mut self, value: String) {
        self.items.push(value);
    }

    /// Get the number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if the builder is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Consume the builder and return the string vector (`g_strv_builder_unref_to_strv`).
    pub fn into_vec(self) -> Vec<String> {
        self.items
    }

    /// Get a reference to the items.
    pub fn items(&self) -> &[String] {
        &self.items
    }
}

impl Default for StrvBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_consume() {
        let mut b = StrvBuilder::new();
        b.add("hello");
        b.add("world");
        let v = b.into_vec();
        assert_eq!(v, vec!["hello".to_owned(), "world".to_owned()]);
    }

    #[test]
    fn addv() {
        let mut b = StrvBuilder::new();
        b.addv(&["a", "b", "c"]);
        assert_eq!(b.len(), 3);
        let v = b.into_vec();
        assert_eq!(v, vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);
    }

    #[test]
    fn take() {
        let mut b = StrvBuilder::new();
        b.take("owned".to_owned());
        let v = b.into_vec();
        assert_eq!(v, vec!["owned".to_owned()]);
    }

    #[test]
    fn empty() {
        let b = StrvBuilder::new();
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
    }
}
