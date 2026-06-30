//! Cache matching `gcache.h` (deprecated).
//!
//! A simple key-value cache with reference counting. Deprecated in GLib 2.26
//! in favor of GHashTable. Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::string::String;

/// Function type for creating a new value from a key.
pub type CacheNewFunc = fn(&str) -> String;
/// Function type for duplicating a value.
pub type CacheDupFunc = fn(&str) -> String;
/// Function type for destroying a value (no-op in Rust).
pub type CacheDestroyFunc = fn(&str);

/// A cache (`GCache`).
///
/// A simple key-value cache. Deprecated in GLib 2.26.
pub struct Cache {
    new_func: CacheNewFunc,
    destroy_func: CacheDestroyFunc,
    key_dup_func: CacheDupFunc,
    entries: BTreeMap<String, (String, u32)>,
}

impl Cache {
    /// Create a new cache (`g_cache_new`).
    pub fn new(
        new_func: CacheNewFunc,
        destroy_func: CacheDestroyFunc,
        key_dup_func: CacheDupFunc,
    ) -> Self {
        Self {
            new_func,
            destroy_func,
            key_dup_func,
            entries: BTreeMap::new(),
        }
    }

    /// Insert a key and get/create the value (`g_cache_insert`).
    ///
    /// If the key exists, increments the reference count.
    /// Otherwise, creates a new value using `new_func`.
    pub fn insert(&mut self, key: &str) -> &str {
        let dup_key = (self.key_dup_func)(key);
        if let Some((_value, count)) = self.entries.get_mut(&dup_key) {
            *count += 1;
        } else {
            let value = (self.new_func)(&dup_key);
            self.entries.insert(dup_key.clone(), (value, 1));
        }
        &self.entries.get(&dup_key).unwrap().0
    }

    /// Remove a reference to a value (`g_cache_remove`).
    ///
    /// Decrements the reference count. When it reaches zero,
    /// the entry is removed.
    pub fn remove(&mut self, value: &str) {
        let mut to_remove = None;
        for (key, (v, count)) in self.entries.iter_mut() {
            if v == value {
                *count -= 1;
                if *count == 0 {
                    (self.destroy_func)(v);
                    to_remove = Some(key.clone());
                }
                break;
            }
        }
        if let Some(key) = to_remove {
            self.entries.remove(&key);
        }
    }

    /// Iterate over keys (`g_cache_key_foreach`).
    pub fn key_foreach<F: FnMut(&str, &str)>(&self, mut f: F) {
        for (key, (value, _)) in &self.entries {
            f(key, value);
        }
    }

    /// Iterate over values (`g_cache_value_foreach`).
    pub fn value_foreach<F: FnMut(&str, &str)>(&self, mut f: F) {
        for (key, (value, _)) in &self.entries {
            f(value, key);
        }
    }

    /// Get the number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries (`g_cache_destroy`).
    pub fn clear(&mut self) {
        for (_, (value, _)) in self.entries.iter() {
            (self.destroy_func)(value);
        }
        self.entries.clear();
    }
}

fn default_new(key: &str) -> String {
    key.to_owned()
}
fn default_dup(key: &str) -> String {
    key.to_owned()
}
fn default_destroy(_: &str) {}

impl Default for Cache {
    fn default() -> Self {
        Self::new(default_new, default_destroy, default_dup)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_uppercase(key: &str) -> String {
        key.to_uppercase()
    }
    fn noop_destroy(_: &str) {}
    fn identity_dup(key: &str) -> String {
        key.to_owned()
    }

    #[test]
    fn insert_and_get() {
        let mut cache = Cache::new(make_uppercase, noop_destroy, identity_dup);
        let val = cache.insert("hello");
        assert_eq!(val, "HELLO");
    }

    #[test]
    fn ref_counting() {
        let mut cache = Cache::new(make_uppercase, noop_destroy, identity_dup);
        cache.insert("foo");
        cache.insert("foo");
        assert_eq!(cache.len(), 1);
        cache.remove("FOO");
        assert_eq!(cache.len(), 1);
        cache.remove("FOO");
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn different_keys() {
        let mut cache = Cache::new(make_uppercase, noop_destroy, identity_dup);
        cache.insert("a");
        cache.insert("b");
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn key_foreach() {
        let mut cache = Cache::new(make_uppercase, noop_destroy, identity_dup);
        cache.insert("x");
        cache.insert("y");
        let mut keys = Vec::new();
        cache.key_foreach(|k, _v| keys.push(k.to_owned()));
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn clear() {
        let mut cache = Cache::new(make_uppercase, noop_destroy, identity_dup);
        cache.insert("a");
        cache.insert("b");
        cache.clear();
        assert!(cache.is_empty());
    }
}
