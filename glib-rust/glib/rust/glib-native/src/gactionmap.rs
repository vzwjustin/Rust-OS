//! GActionMap interface matching `gio/gactionmap.h`.
//!
//! Upstream `GActionMap` is a `GInterface` for objects that map action
//! names to `GAction` instances. We port it as a Rust trait with an
//! `ActionEntry` struct for batch registration.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gaction::Action;
use crate::gsimpleaction::SimpleAction;
use crate::variant::Variant;
use alloc::boxed::Box;
use alloc::string::{String, ToString};

/// An action entry for batch registration (`GActionEntry`).
///
/// Describes an action to be added to an `ActionMap` via
/// `add_action_entries`.
#[derive(Clone)]
pub struct ActionEntry {
    pub name: String,
    pub parameter_type: Option<String>,
    pub state: Option<String>,
}

impl ActionEntry {
    /// Creates a new stateless action entry with no parameter type.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            parameter_type: None,
            state: None,
        }
    }

    /// Creates a new action entry with a parameter type.
    pub fn with_parameter(name: &str, parameter_type: &str) -> Self {
        Self {
            name: name.to_string(),
            parameter_type: Some(parameter_type.to_string()),
            state: None,
        }
    }

    /// Creates a new stateful action entry.
    pub fn with_state(name: &str, state: &str) -> Self {
        Self {
            name: name.to_string(),
            parameter_type: None,
            state: Some(state.to_string()),
        }
    }
}

/// Trait for action maps (`GActionMap`).
pub trait ActionMap {
    /// Looks up an action by name.
    ///
    /// Mirrors `g_action_map_lookup_action`.
    fn lookup_action(&self, action_name: &str) -> Option<&dyn Action>;

    /// Adds an action to the map.
    ///
    /// Mirrors `g_action_map_add_action`.
    fn add_action(&self, action: Box<dyn Action>);

    /// Removes an action from the map by name.
    ///
    /// Mirrors `g_action_map_remove_action`.
    fn remove_action(&self, action_name: &str);

    /// Adds multiple actions from entries.
    ///
    /// Mirrors `g_action_map_add_action_entries`.
    fn add_action_entries(&self, entries: &[ActionEntry]) {
        for entry in entries {
            let action = if let Some(ref state) = entry.state {
                Box::new(SimpleAction::new_stateful(
                    &entry.name,
                    None,
                    Variant::new_string(state),
                )) as Box<dyn Action>
            } else {
                Box::new(SimpleAction::new(&entry.name, None)) as Box<dyn Action>
            };
            self.add_action(action);
        }
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gaction::Action;
    use crate::gsimpleaction::SimpleAction;
    use crate::variant::Variant;
    use spin::Mutex;

    struct TestActionMap {
        actions: Mutex<Vec<(String, Box<dyn Action>)>>,
    }

    impl TestActionMap {
        fn new() -> Self {
            Self {
                actions: Mutex::new(Vec::new()),
            }
        }
    }

    impl ActionMap for TestActionMap {
        fn lookup_action(&self, action_name: &str) -> Option<&dyn Action> {
            let actions = self.actions.lock();
            let found = actions.iter().find(|(n, _)| n == action_name)?;
            // SAFETY: We're returning a reference to the boxed action inside
            // the Mutex. This is safe because the Mutex guard is dropped but
            // the data remains valid as long as the map exists. In test context
            // this is fine since we don't modify while holding the reference.
            unsafe {
                let ptr = &*found.1 as *const dyn Action;
                Some(&*ptr)
            }
        }

        fn add_action(&self, action: Box<dyn Action>) {
            let name = action.get_name().to_string();
            self.actions.lock().push((name, action));
        }

        fn remove_action(&self, action_name: &str) {
            self.actions.lock().retain(|(n, _)| n != action_name);
        }
    }

    #[test]
    fn test_action_map_add_lookup() {
        let map = TestActionMap::new();
        map.add_action(Box::new(SimpleAction::new("open", None)));
        let action = map.lookup_action("open");
        assert!(action.is_some());
        assert_eq!(action.unwrap().get_name(), "open");
    }

    #[test]
    fn test_action_map_lookup_nonexistent() {
        let map = TestActionMap::new();
        assert!(map.lookup_action("nonexistent").is_none());
    }

    #[test]
    fn test_action_map_remove() {
        let map = TestActionMap::new();
        map.add_action(Box::new(SimpleAction::new("save", None)));
        assert!(map.lookup_action("save").is_some());
        map.remove_action("save");
        assert!(map.lookup_action("save").is_none());
    }

    #[test]
    fn test_action_map_add_multiple() {
        let map = TestActionMap::new();
        map.add_action(Box::new(SimpleAction::new("open", None)));
        map.add_action(Box::new(SimpleAction::new("save", None)));
        map.add_action(Box::new(SimpleAction::new("close", None)));
        assert!(map.lookup_action("open").is_some());
        assert!(map.lookup_action("save").is_some());
        assert!(map.lookup_action("close").is_some());
    }

    #[test]
    fn test_action_entry_new() {
        let entry = ActionEntry::new("open");
        assert_eq!(entry.name, "open");
        assert!(entry.parameter_type.is_none());
        assert!(entry.state.is_none());
    }

    #[test]
    fn test_action_entry_with_state() {
        let entry = ActionEntry::with_state("toggle", "true");
        assert_eq!(entry.name, "toggle");
        assert_eq!(entry.state.as_deref(), Some("true"));
    }

    #[test]
    fn test_add_action_entries() {
        let map = TestActionMap::new();
        let entries = vec![
            ActionEntry::new("open"),
            ActionEntry::new("save"),
            ActionEntry::with_state("toggle", "false"),
        ];
        map.add_action_entries(&entries);
        assert!(map.lookup_action("open").is_some());
        assert!(map.lookup_action("save").is_some());
        assert!(map.lookup_action("toggle").is_some());
    }
}
