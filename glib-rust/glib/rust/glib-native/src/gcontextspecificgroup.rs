//! GContextSpecificGroup matching `gio/gcontextspecificgroup.h`.
//! A group of context-specific objects. In this no_std port we model it
//! as a simple registry keyed by string.
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A context-specific group (`GContextSpecificGroup`).
pub struct ContextSpecificGroup {
    members: Mutex<BTreeMap<String, String>>,
}

impl ContextSpecificGroup {
    pub fn new() -> Self {
        Self {
            members: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn add(&self, key: &str, value: &str) {
        self.members
            .lock()
            .insert(key.to_string(), value.to_string());
    }

    pub fn remove(&self, key: &str) -> bool {
        self.members.lock().remove(key).is_some()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.members.lock().get(key).cloned()
    }

    pub fn keys(&self) -> Vec<String> {
        self.members.lock().keys().cloned().collect()
    }

    pub fn count(&self) -> usize {
        self.members.lock().len()
    }
}

impl Default for ContextSpecificGroup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_remove() {
        let g = ContextSpecificGroup::new();
        g.add("ctx1", "value1");
        g.add("ctx2", "value2");
        assert_eq!(g.count(), 2);
        assert_eq!(g.get("ctx1"), Some("value1".to_string()));
        assert!(g.remove("ctx1"));
        assert_eq!(g.count(), 1);
    }
}
