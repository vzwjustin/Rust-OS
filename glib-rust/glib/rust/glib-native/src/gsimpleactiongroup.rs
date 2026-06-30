//! GSimpleActionGroup matching `gio/gsimpleactiongroup.h`.
//!
//! Upstream `GSimpleActionGroup` is a concrete implementation of both
//! `GActionGroup` and `GActionMap`. We port it as a struct with
//! `Mutex`-protected `Vec` of named actions.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gaction::Action;
use crate::gactiongroup::{ActionGroup, ActionInfo};
use crate::gactionmap::ActionMap;
use crate::variant::Variant;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A simple action group (`GSimpleActionGroup`).
///
/// Implements both `ActionGroup` and `ActionMap` with `Vec` storage.
pub struct SimpleActionGroup {
    actions: Mutex<Vec<(String, Box<dyn Action>)>>,
}

impl SimpleActionGroup {
    /// Creates a new empty action group.
    ///
    /// Mirrors `g_simple_action_group_new`.
    pub fn new() -> Self {
        Self {
            actions: Mutex::new(Vec::new()),
        }
    }

    fn find_index(&self, action_name: &str) -> Option<usize> {
        self.actions
            .lock()
            .iter()
            .position(|(n, _)| n == action_name)
    }
}

impl Default for SimpleActionGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionGroup for SimpleActionGroup {
    fn has_action(&self, action_name: &str) -> bool {
        self.find_index(action_name).is_some()
    }

    fn list_actions(&self) -> Vec<String> {
        self.actions.lock().iter().map(|(n, _)| n.clone()).collect()
    }

    fn query_action(&self, action_name: &str) -> Option<ActionInfo> {
        let idx = self.find_index(action_name)?;
        let actions = self.actions.lock();
        let (_, action) = &actions[idx];
        Some(ActionInfo {
            enabled: action.get_enabled(),
            parameter_type: action.get_parameter_type().cloned(),
            state_type: action.get_state_type().cloned(),
            state_hint: action.get_state_hint(),
            state: action.get_state(),
        })
    }

    fn change_action_state(&self, action_name: &str, value: Variant) {
        if let Some(idx) = self.find_index(action_name) {
            let actions = self.actions.lock();
            let (_, action) = &actions[idx];
            action.change_state(value);
        }
    }

    fn activate_action(&self, action_name: &str, parameter: Option<Variant>) {
        if let Some(idx) = self.find_index(action_name) {
            let actions = self.actions.lock();
            let (_, action) = &actions[idx];
            action.activate(parameter);
        }
    }
}

impl ActionMap for SimpleActionGroup {
    fn lookup_action(&self, action_name: &str) -> Option<&dyn Action> {
        let actions = self.actions.lock();
        let found = actions.iter().find(|(n, _)| n == action_name)?;
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

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_action_group_new() {
        let group = SimpleActionGroup::new();
        assert!(!group.has_action("test"));
        assert_eq!(group.list_actions().len(), 0);
    }

    #[test]
    fn test_add_lookup_action() {
        let group = SimpleActionGroup::new();
        group.add_action(Box::new(SimpleAction::new("open", None)));
        assert!(group.has_action("open"));
        assert!(group.lookup_action("open").is_some());
        assert_eq!(group.lookup_action("open").unwrap().get_name(), "open");
    }

    #[test]
    fn test_remove_action() {
        let group = SimpleActionGroup::new();
        group.add_action(Box::new(SimpleAction::new("save", None)));
        assert!(group.has_action("save"));
        group.remove_action("save");
        assert!(!group.has_action("save"));
    }

    #[test]
    fn test_list_actions() {
        let group = SimpleActionGroup::new();
        group.add_action(Box::new(SimpleAction::new("open", None)));
        group.add_action(Box::new(SimpleAction::new("save", None)));
        group.add_action(Box::new(SimpleAction::new("close", None)));
        let actions = group.list_actions();
        assert_eq!(actions.len(), 3);
        assert!(actions.contains(&"open".to_string()));
        assert!(actions.contains(&"save".to_string()));
        assert!(actions.contains(&"close".to_string()));
    }

    #[test]
    fn test_query_action() {
        let group = SimpleActionGroup::new();
        group.add_action(Box::new(SimpleAction::new_stateful(
            "toggle",
            None,
            Variant::new_boolean(true),
        )));
        let info = group.query_action("toggle").unwrap();
        assert!(info.enabled);
        assert!(info.state.is_some());
        assert_eq!(info.state.unwrap().get_boolean(), true);
    }

    #[test]
    fn test_change_action_state() {
        let group = SimpleActionGroup::new();
        group.add_action(Box::new(SimpleAction::new_stateful(
            "toggle",
            None,
            Variant::new_boolean(true),
        )));
        group.change_action_state("toggle", Variant::new_boolean(false));
        assert_eq!(
            group.get_action_state("toggle").unwrap().get_boolean(),
            false
        );
    }

    #[test]
    fn test_activate_action() {
        let group = SimpleActionGroup::new();
        group.add_action(Box::new(SimpleAction::new("click", None)));
        group.activate_action("click", None);
    }

    #[test]
    fn test_add_action_entries() {
        let group = SimpleActionGroup::new();
        let entries = vec![
            ActionEntry::new("open"),
            ActionEntry::new("save"),
            ActionEntry::with_state("toggle", "false"),
        ];
        group.add_action_entries(&entries);
        assert!(group.has_action("open"));
        assert!(group.has_action("save"));
        assert!(group.has_action("toggle"));
    }

    #[test]
    fn test_query_nonexistent() {
        let group = SimpleActionGroup::new();
        assert!(group.query_action("nonexistent").is_none());
        assert!(!group.get_action_enabled("nonexistent"));
    }
}
