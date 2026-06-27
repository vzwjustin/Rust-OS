//! Completion matching `gcompletion.h` / `gcompletion.c` (deprecated).
//!
//! Prefix-based completion over a list of items. Fully `no_std` compatible
//! using `alloc`.

use crate::prelude::*;

/// A completion function that extracts a string from an item.
pub type CompletionFunc<T> = fn(&T) -> String;

/// A comparison function for completion matching.
pub type CompletionStrncmpFunc = fn(&str, &str, usize) -> core::cmp::Ordering;

/// A completion (`GCompletion`).
///
/// Provides prefix-based completion over a collection of items.
/// Deprecated in GLib 2.26 but useful for simple autocomplete.
pub struct Completion<T> {
    items: Vec<T>,
    func: CompletionFunc<T>,
    strncmp_func: CompletionStrncmpFunc,
}

impl<T> Completion<T> {
    /// Create a new completion (`g_completion_new`).
    pub fn new(func: CompletionFunc<T>) -> Self {
        Self {
            items: Vec::new(),
            func,
            strncmp_func: default_strncmp,
        }
    }

    /// Add items (`g_completion_add_items`).
    pub fn add_items(&mut self, items: Vec<T>) {
        self.items.extend(items);
    }

    /// Remove items by predicate (`g_completion_remove_items`).
    pub fn remove_items(&mut self, mut pred: impl FnMut(&T) -> bool) {
        self.items.retain(|item| !pred(item));
    }

    /// Clear all items (`g_completion_clear_items`).
    pub fn clear_items(&mut self) {
        self.items.clear();
    }

    /// Complete a prefix (`g_completion_complete`).
    ///
    /// Returns the list of matching items and the common prefix.
    pub fn complete(&self, prefix: &str) -> (Vec<&T>, String) {
        let mut matches: Vec<&T> = Vec::new();
        for item in &self.items {
            let s = (self.func)(item);
            if s.starts_with(prefix) {
                matches.push(item);
            }
        }

        let common_prefix = if matches.is_empty() {
            String::new()
        } else if matches.len() == 1 {
            (self.func)(matches[0])
        } else {
            // Find common prefix among all matches
            let first = (self.func)(matches[0]);
            let mut common_len = first.len();
            for item in &matches[1..] {
                let s = (self.func)(item);
                common_len = common_len.min(s.len());
                let mut i = 0;
                while i < common_len {
                    if first.as_bytes()[i] != s.as_bytes()[i] {
                        common_len = i;
                        break;
                    }
                    i += 1;
                }
            }
            first[..common_len].to_owned()
        };

        (matches, common_prefix)
    }

    /// Set the comparison function (`g_completion_set_compare`).
    pub fn set_compare(&mut self, func: CompletionStrncmpFunc) {
        self.strncmp_func = func;
    }

    /// Get the number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if there are no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

fn default_strncmp(s1: &str, s2: &str, n: usize) -> core::cmp::Ordering {
    let b1 = s1.as_bytes();
    let b2 = s2.as_bytes();
    let len = n.min(b1.len()).min(b2.len());
    for i in 0..len {
        match b1[i].cmp(&b2[i]) {
            core::cmp::Ordering::Equal => continue,
            ord => return ord,
        }
    }
    b1.len().min(n).cmp(&b2.len().min(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item_func(item: &String) -> String {
        item.clone()
    }

    #[test]
    fn basic_completion() {
        let mut c = Completion::new(item_func);
        c.add_items(vec![
            "apple".to_owned(),
            "application".to_owned(),
            "banana".to_owned(),
            "apricot".to_owned(),
        ]);

        let (matches, prefix) = c.complete("ap");
        assert_eq!(matches.len(), 3);
        assert_eq!(prefix, "ap");
    }

    #[test]
    fn single_match() {
        let mut c = Completion::new(item_func);
        c.add_items(vec!["apple".to_owned(), "banana".to_owned()]);
        let (matches, prefix) = c.complete("ban");
        assert_eq!(matches.len(), 1);
        assert_eq!(prefix, "banana");
    }

    #[test]
    fn no_match() {
        let mut c = Completion::new(item_func);
        c.add_items(vec!["apple".to_owned()]);
        let (matches, prefix) = c.complete("xyz");
        assert!(matches.is_empty());
        assert_eq!(prefix, "");
    }

    #[test]
    fn common_prefix() {
        let mut c = Completion::new(item_func);
        c.add_items(vec!["apple".to_owned(), "apricot".to_owned()]);
        let (matches, prefix) = c.complete("ap");
        assert_eq!(matches.len(), 2);
        assert_eq!(prefix, "ap");
    }

    #[test]
    fn clear_items() {
        let mut c = Completion::new(item_func);
        c.add_items(vec!["a".to_owned(), "b".to_owned()]);
        c.clear_items();
        assert_eq!(c.len(), 0);
        assert!(c.is_empty());
    }

    #[test]
    fn remove_items() {
        let mut c = Completion::new(item_func);
        c.add_items(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);
        c.remove_items(|s| s == "b");
        assert_eq!(c.len(), 2);
    }
}
