//! GActionGroup interface matching `gio/gactiongroup.h`.
//!
//! Upstream `GActionGroup` is a `GInterface` for groups of actions.
//! We port it as a Rust trait.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::variant::Variant;
use crate::varianttype::VariantType;
use alloc::string::String;
use alloc::vec::Vec;

/// Result of querying an action's properties.
#[derive(Clone, Debug)]
pub struct ActionInfo {
    pub enabled: bool,
    pub parameter_type: Option<VariantType>,
    pub state_type: Option<VariantType>,
    pub state_hint: Option<Variant>,
    pub state: Option<Variant>,
}

/// Trait for action groups (`GActionGroup`).
pub trait ActionGroup {
    /// Checks if the group has an action with the given name.
    ///
    /// Mirrors `g_action_group_has_action`.
    fn has_action(&self, action_name: &str) -> bool;

    /// Lists the names of all actions in the group.
    ///
    /// Mirrors `g_action_group_list_actions`.
    fn list_actions(&self) -> Vec<String>;

    /// Queries all properties of an action at once.
    ///
    /// Mirrors `g_action_group_query_action`.
    /// Returns `None` if the action doesn't exist.
    fn query_action(&self, action_name: &str) -> Option<ActionInfo>;

    /// Gets whether the action is enabled.
    ///
    /// Mirrors `g_action_group_get_action_enabled`.
    fn get_action_enabled(&self, action_name: &str) -> bool {
        self.query_action(action_name)
            .map(|info| info.enabled)
            .unwrap_or(false)
    }

    /// Gets the parameter type of the action.
    ///
    /// Mirrors `g_action_group_get_action_parameter_type`.
    fn get_action_parameter_type(&self, action_name: &str) -> Option<VariantType> {
        self.query_action(action_name)
            .and_then(|info| info.parameter_type)
    }

    /// Gets the state type of the action.
    ///
    /// Mirrors `g_action_group_get_action_state_type`.
    fn get_action_state_type(&self, action_name: &str) -> Option<VariantType> {
        self.query_action(action_name)
            .and_then(|info| info.state_type)
    }

    /// Gets the state hint of the action.
    ///
    /// Mirrors `g_action_group_get_action_state_hint`.
    fn get_action_state_hint(&self, action_name: &str) -> Option<Variant> {
        self.query_action(action_name)
            .and_then(|info| info.state_hint)
    }

    /// Gets the state of the action.
    ///
    /// Mirrors `g_action_group_get_action_state`.
    fn get_action_state(&self, action_name: &str) -> Option<Variant> {
        self.query_action(action_name).and_then(|info| info.state)
    }

    /// Requests a state change on the action.
    ///
    /// Mirrors `g_action_group_change_action_state`.
    fn change_action_state(&self, action_name: &str, value: Variant);

    /// Activates the action.
    ///
    /// Mirrors `g_action_group_activate_action`.
    fn activate_action(&self, action_name: &str, parameter: Option<Variant>);
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gaction::Action;
    use crate::gsimpleaction::SimpleAction;
    use alloc::string::ToString;
    use alloc::vec::Vec;
    use spin::Mutex;

    struct SimpleActionGroup {
        actions: Mutex<Vec<SimpleAction>>,
        names: Mutex<Vec<String>>,
    }

    impl SimpleActionGroup {
        fn new() -> Self {
            Self {
                actions: Mutex::new(Vec::new()),
                names: Mutex::new(Vec::new()),
            }
        }

        fn add(&self, name: &str, action: SimpleAction) {
            self.names.lock().push(name.to_string());
            self.actions.lock().push(action);
        }

        fn find_index(&self, action_name: &str) -> Option<usize> {
            self.names.lock().iter().position(|n| n == action_name)
        }
    }

    impl ActionGroup for SimpleActionGroup {
        fn has_action(&self, action_name: &str) -> bool {
            self.find_index(action_name).is_some()
        }

        fn list_actions(&self) -> Vec<String> {
            self.names.lock().clone()
        }

        fn query_action(&self, action_name: &str) -> Option<ActionInfo> {
            let idx = self.find_index(action_name)?;
            let actions = self.actions.lock();
            let action = &actions[idx];
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
                actions[idx].change_state(value);
            }
        }

        fn activate_action(&self, action_name: &str, parameter: Option<Variant>) {
            if let Some(idx) = self.find_index(action_name) {
                let actions = self.actions.lock();
                actions[idx].activate(parameter);
            }
        }
    }

    #[test]
    fn test_action_group_has_action() {
        let group = SimpleActionGroup::new();
        group.add("open", SimpleAction::new("open", None));
        assert!(group.has_action("open"));
        assert!(!group.has_action("save"));
    }

    #[test]
    fn test_action_group_list_actions() {
        let group = SimpleActionGroup::new();
        group.add("open", SimpleAction::new("open", None));
        group.add("save", SimpleAction::new("save", None));
        let actions = group.list_actions();
        assert_eq!(actions.len(), 2);
        assert!(actions.contains(&"open".to_string()));
        assert!(actions.contains(&"save".to_string()));
    }

    #[test]
    fn test_action_group_query_action() {
        let group = SimpleActionGroup::new();
        group.add(
            "toggle",
            SimpleAction::new_stateful("toggle", None, Variant::new_boolean(true)),
        );
        let info = group.query_action("toggle").unwrap();
        assert!(info.enabled);
        assert!(info.state.is_some());
        assert_eq!(info.state.unwrap().get_boolean(), true);
    }

    #[test]
    fn test_action_group_get_action_enabled() {
        let group = SimpleActionGroup::new();
        let action = SimpleAction::new("save", None);
        action.set_enabled(false);
        group.add("save", action);
        assert!(!group.get_action_enabled("save"));
    }

    #[test]
    fn test_action_group_change_action_state() {
        let group = SimpleActionGroup::new();
        group.add(
            "toggle",
            SimpleAction::new_stateful("toggle", None, Variant::new_boolean(true)),
        );
        group.change_action_state("toggle", Variant::new_boolean(false));
        assert_eq!(
            group.get_action_state("toggle").unwrap().get_boolean(),
            false
        );
    }

    #[test]
    fn test_action_group_activate_action() {
        let group = SimpleActionGroup::new();
        group.add("click", SimpleAction::new("click", None));
        group.activate_action("click", None);
        // No crash = success (activate is a no-op on SimpleAction)
    }

    #[test]
    fn test_action_group_query_nonexistent() {
        let group = SimpleActionGroup::new();
        assert!(group.query_action("nonexistent").is_none());
        assert!(!group.get_action_enabled("nonexistent"));
    }
}
