//! GDBusActionGroup matching `gio/gdbusactiongroup.h`.
//!
//! An action group backed by D-Bus. In this no_std port we model it
//! as a wrapper around `RemoteActionGroup` with a D-Bus connection path.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

use crate::gremoteactiongroup::{RemoteAction, RemoteActionGroup};

/// A D-Bus-backed action group (`GDBusActionGroup`).
pub struct DBusActionGroup {
    bus_name: Mutex<String>,
    object_path: Mutex<String>,
    inner: RemoteActionGroup,
}

impl DBusActionGroup {
    /// Creates a new D-Bus action group.
    pub fn new(bus_name: &str, object_path: &str) -> Self {
        Self {
            bus_name: Mutex::new(bus_name.to_string()),
            object_path: Mutex::new(object_path.to_string()),
            inner: RemoteActionGroup::new(),
        }
    }

    /// Gets the bus name.
    pub fn get_bus_name(&self) -> String {
        self.bus_name.lock().clone()
    }

    /// Gets the object path.
    pub fn get_object_path(&self) -> String {
        self.object_path.lock().clone()
    }

    /// Adds an action.
    pub fn add_action(&self, action: RemoteAction) {
        self.inner.add_action(action);
    }

    /// Lists all actions.
    pub fn list_actions(&self) -> Vec<String> {
        self.inner.list_actions()
    }

    /// Activates an action.
    pub fn activate_action(&self, name: &str, parameter: Option<&str>) -> bool {
        self.inner.activate_action(name, parameter)
    }

    /// Returns the action count.
    pub fn action_count(&self) -> usize {
        self.inner.action_count()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let g = DBusActionGroup::new("org.test.Bus", "/org/test/actions");
        assert_eq!(g.get_bus_name(), "org.test.Bus");
        assert_eq!(g.get_object_path(), "/org/test/actions");
        assert_eq!(g.action_count(), 0);
    }

    #[test]
    fn test_add_activate() {
        let g = DBusActionGroup::new("org.test", "/actions");
        g.add_action(RemoteAction {
            name: "quit".to_string(),
            enabled: true,
            parameter_type: None,
            state: None,
        });
        assert_eq!(g.action_count(), 1);
        assert!(g.activate_action("quit", None));
    }
}
