//! GPropertyAction matching `gio/gpropertyaction.h`.
//!
//! Upstream `GPropertyAction` is a concrete `GAction` implementation
//! bound to a property on an object. We port it as a struct implementing
//! the `Action` trait, using a `Mutex`-protected `Variant` state to
//! simulate the property binding.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gaction::Action;
use crate::variant::Variant;
use crate::varianttype::VariantType;
use alloc::string::{String, ToString};
use spin::Mutex;

/// A property action (`GPropertyAction`).
///
/// Wraps a named property as an `Action`. The state reflects the
/// property value, and `change_state` updates it.
pub struct PropertyAction {
    name: String,
    property_name: String,
    enabled: Mutex<bool>,
    state: Mutex<Variant>,
    state_type: VariantType,
}

impl PropertyAction {
    /// Creates a new property action.
    ///
    /// Mirrors `g_property_action_new`.
    pub fn new(name: &str, property_name: &str, initial_state: Variant) -> Self {
        let state_type = VariantType::new(initial_state.type_string())
            .unwrap_or_else(|| VariantType::new("v").unwrap());
        Self {
            name: name.to_string(),
            property_name: property_name.to_string(),
            enabled: Mutex::new(true),
            state: Mutex::new(initial_state),
            state_type,
        }
    }

    /// Gets the property name this action is bound to.
    pub fn get_property_name(&self) -> &str {
        &self.property_name
    }
}

impl Action for PropertyAction {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_enabled(&self) -> bool {
        *self.enabled.lock()
    }

    fn get_parameter_type(&self) -> Option<&VariantType> {
        None
    }

    fn get_state_type(&self) -> Option<&VariantType> {
        Some(&self.state_type)
    }

    fn get_state_hint(&self) -> Option<Variant> {
        None
    }

    fn get_state(&self) -> Option<Variant> {
        Some(self.state.lock().clone())
    }

    fn change_state(&self, value: Variant) {
        *self.state.lock() = value;
    }

    fn activate(&self, _parameter: Option<Variant>) {
        // Property actions are stateful; activate is a no-op
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_property_action_new() {
        let action = PropertyAction::new("visible", "visible", Variant::new_boolean(true));
        assert_eq!(action.get_name(), "visible");
        assert_eq!(action.get_property_name(), "visible");
        assert!(action.get_enabled());
    }

    #[test]
    fn test_property_action_state() {
        let action = PropertyAction::new("volume", "volume", Variant::new_int32(50));
        assert_eq!(action.get_state().unwrap().get_int32(), 50);
    }

    #[test]
    fn test_property_action_change_state() {
        let action = PropertyAction::new("volume", "volume", Variant::new_int32(50));
        action.change_state(Variant::new_int32(75));
        assert_eq!(action.get_state().unwrap().get_int32(), 75);
    }

    #[test]
    fn test_property_action_state_type() {
        let action = PropertyAction::new("visible", "visible", Variant::new_boolean(true));
        assert!(action.get_state_type().is_some());
    }

    #[test]
    fn test_property_action_no_parameter_type() {
        let action = PropertyAction::new("visible", "visible", Variant::new_boolean(true));
        assert!(action.get_parameter_type().is_none());
    }

    #[test]
    fn test_property_action_activate_noop() {
        let action = PropertyAction::new("visible", "visible", Variant::new_boolean(true));
        action.activate(None);
        // State should be unchanged
        assert_eq!(action.get_state().unwrap().get_boolean(), true);
    }

    #[test]
    fn test_property_action_string_state() {
        let action = PropertyAction::new("name", "name", Variant::new_string("hello"));
        assert_eq!(action.get_state().unwrap().get_string(), "hello");
        action.change_state(Variant::new_string("world"));
        assert_eq!(action.get_state().unwrap().get_string(), "world");
    }
}
