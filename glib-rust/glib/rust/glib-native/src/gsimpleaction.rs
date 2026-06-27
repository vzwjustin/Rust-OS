//! GSimpleAction matching `gio/gsimpleaction.h` / `gsimpleaction.c`.
//!
//! Upstream `GSimpleAction` is a `GAction` implementation that stores
//! its own name, parameter type, state, and enabled flag. We port it
//! as a plain Rust struct with `Mutex`-protected state.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gaction::Action;
use crate::variant::Variant;
use crate::varianttype::VariantType;
use alloc::string::{String, ToString};
use spin::Mutex;

struct SimpleActionState {
    enabled: bool,
    state: Option<Variant>,
    state_hint: Option<Variant>,
}

/// A simple action (`GSimpleAction`).
///
/// A concrete implementation of `Action` that stores its own state.
pub struct SimpleAction {
    name: String,
    parameter_type: Option<VariantType>,
    state_type: Option<VariantType>,
    state: Mutex<SimpleActionState>,
}

impl SimpleAction {
    /// Creates a new stateless action.
    ///
    /// Mirrors `g_simple_action_new`.
    pub fn new(name: &str, parameter_type: Option<VariantType>) -> Self {
        Self {
            name: name.to_string(),
            parameter_type,
            state_type: None,
            state: Mutex::new(SimpleActionState {
                enabled: true,
                state: None,
                state_hint: None,
            }),
        }
    }

    /// Creates a new stateful action.
    ///
    /// Mirrors `g_simple_action_new_stateful`.
    pub fn new_stateful(name: &str, parameter_type: Option<VariantType>, state: Variant) -> Self {
        let state_type = VariantType::new(state.type_string());
        Self {
            name: name.to_string(),
            parameter_type,
            state_type,
            state: Mutex::new(SimpleActionState {
                enabled: true,
                state: Some(state),
                state_hint: None,
            }),
        }
    }

    /// Sets whether the action is enabled.
    ///
    /// Mirrors `g_simple_action_set_enabled`.
    pub fn set_enabled(&self, enabled: bool) {
        self.state.lock().enabled = enabled;
    }

    /// Sets the state of the action.
    ///
    /// Mirrors `g_simple_action_set_state`.
    pub fn set_state(&self, value: Variant) {
        self.state.lock().state = Some(value);
    }

    /// Sets the state hint.
    ///
    /// Mirrors `g_simple_action_set_state_hint`.
    pub fn set_state_hint(&self, state_hint: Option<Variant>) {
        self.state.lock().state_hint = state_hint;
    }
}

impl Action for SimpleAction {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_parameter_type(&self) -> Option<&VariantType> {
        self.parameter_type.as_ref()
    }

    fn get_state_type(&self) -> Option<&VariantType> {
        self.state_type.as_ref()
    }

    fn get_state_hint(&self) -> Option<Variant> {
        self.state.lock().state_hint.clone()
    }

    fn get_enabled(&self) -> bool {
        self.state.lock().enabled
    }

    fn get_state(&self) -> Option<Variant> {
        self.state.lock().state.clone()
    }

    fn change_state(&self, value: Variant) {
        self.set_state(value);
    }

    fn activate(&self, _parameter: Option<Variant>) {
        // SimpleAction doesn't do anything on activate by default.
        // Upstream emits the "activate" signal; we're a no-op here.
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_action_new() {
        let action = SimpleAction::new("open", None);
        assert_eq!(action.get_name(), "open");
        assert!(action.get_parameter_type().is_none());
        assert!(action.get_state().is_none());
        assert!(action.get_enabled());
    }

    #[test]
    fn test_simple_action_new_stateful() {
        let action = SimpleAction::new_stateful("toggle", None, Variant::new_boolean(false));
        assert_eq!(action.get_name(), "toggle");
        assert!(action.get_state().is_some());
        assert_eq!(action.get_state().unwrap().get_boolean(), false);
    }

    #[test]
    fn test_set_enabled() {
        let action = SimpleAction::new("save", None);
        assert!(action.get_enabled());
        action.set_enabled(false);
        assert!(!action.get_enabled());
    }

    #[test]
    fn test_set_state() {
        let action = SimpleAction::new_stateful("volume", None, Variant::new_int64(50));
        assert_eq!(action.get_state().unwrap().get_int64(), 50);
        action.set_state(Variant::new_int64(75));
        assert_eq!(action.get_state().unwrap().get_int64(), 75);
    }

    #[test]
    fn test_change_state() {
        let action = SimpleAction::new_stateful("mode", None, Variant::new_string("auto"));
        assert_eq!(action.get_state().unwrap().get_string(), "auto");
        action.change_state(Variant::new_string("manual"));
        assert_eq!(action.get_state().unwrap().get_string(), "manual");
    }

    #[test]
    fn test_set_state_hint() {
        let action = SimpleAction::new("seek", None);
        assert!(action.get_state_hint().is_none());
        action.set_state_hint(Some(Variant::new_int64(100)));
        assert!(action.get_state_hint().is_some());
    }

    #[test]
    fn test_activate_is_noop() {
        let action = SimpleAction::new("click", None);
        action.activate(None);
        // No crash, no state change
        assert!(action.get_enabled());
    }
}
