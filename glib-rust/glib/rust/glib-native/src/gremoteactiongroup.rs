//! GRemoteActionGroup matching `gio/gremoteactiongroup.h`.
//!
//! An action group that can be triggered remotely. In this no_std port
//! we model it as a trait extending the action group concept.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A remote action group (`GRemoteActionGroup`).
pub struct RemoteActionGroup {
    actions: Mutex<BTreeMap<String, RemoteAction>>,
}

/// A remote action with a parameter type.
#[derive(Debug, Clone)]
pub struct RemoteAction {
    pub name: String,
    pub enabled: bool,
    pub parameter_type: Option<String>,
    pub state: Option<String>,
}

impl RemoteActionGroup {
    /// Creates a new remote action group.
    pub fn new() -> Self {
        Self {
            actions: Mutex::new(BTreeMap::new()),
        }
    }

    /// Adds an action to the group.
    pub fn add_action(&self, action: RemoteAction) {
        self.actions.lock().insert(action.name.clone(), action);
    }

    /// Removes an action by name.
    pub fn remove_action(&self, name: &str) -> bool {
        self.actions.lock().remove(name).is_some()
    }

    /// Gets an action by name.
    pub fn get_action(&self, name: &str) -> Option<RemoteAction> {
        self.actions.lock().get(name).cloned()
    }

    /// Lists all action names.
    pub fn list_actions(&self) -> Vec<String> {
        self.actions.lock().keys().cloned().collect()
    }

    /// Activates an action with optional parameter.
    pub fn activate_action(&self, name: &str, _parameter: Option<&str>) -> bool {
        self.actions.lock().contains_key(name)
    }

    /// Changes the state of an action.
    pub fn change_action_state(&self, name: &str, state: &str) -> bool {
        let mut actions = self.actions.lock();
        if let Some(a) = actions.get_mut(name) {
            a.state = Some(state.to_string());
            true
        } else {
            false
        }
    }

    /// Returns the number of actions.
    pub fn action_count(&self) -> usize {
        self.actions.lock().len()
    }
}

impl Default for RemoteActionGroup {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let g = RemoteActionGroup::new();
        assert_eq!(g.action_count(), 0);
    }

    #[test]
    fn test_add_remove() {
        let g = RemoteActionGroup::new();
        g.add_action(RemoteAction {
            name: "save".to_string(),
            enabled: true,
            parameter_type: None,
            state: None,
        });
        assert_eq!(g.action_count(), 1);
        assert!(g.remove_action("save"));
        assert_eq!(g.action_count(), 0);
    }

    #[test]
    fn test_activate() {
        let g = RemoteActionGroup::new();
        g.add_action(RemoteAction {
            name: "open".to_string(),
            enabled: true,
            parameter_type: Some("s".to_string()),
            state: None,
        });
        assert!(g.activate_action("open", None));
        assert!(!g.activate_action("missing", None));
    }

    #[test]
    fn test_change_state() {
        let g = RemoteActionGroup::new();
        g.add_action(RemoteAction {
            name: "toggle".to_string(),
            enabled: true,
            parameter_type: None,
            state: Some("off".to_string()),
        });
        assert!(g.change_action_state("toggle", "on"));
        assert_eq!(
            g.get_action("toggle").unwrap().state,
            Some("on".to_string())
        );
    }
}
