//! Port of GNOME mutter's `clutter/clutter-accessibility.{c,h}`.
//!
//! Minimal module that initializes the accessibility root object and tracks
//! whether accessibility support is enabled.
//!
//! # What's ported
//!
//! - `AccessibilityState` struct: `enabled`, `initialized`, root object.
//! - `clutter_accessibility_init` / `clutter_accessibility_get_default`.
//! - `clutter_get_accessibility_enabled` / `clutter_disable_accessibility`.
//! - `AccessibilityRoot`: placeholder for the `AtkObject` root.
//! - `AccessibleRole` / `AccessibleState` enums.
//!
//! # What's skipped
//!
//! - `AtkObject` / ATK type system: not available. Modeled as plain structs.
//! - Signal emission, `AtkRegistry` / `AtkObjectFactory`: not ported.

#![allow(dead_code)]

use alloc::string::String;
use alloc::vec::Vec;

/// `AtkRole` — common subset of ATK roles used by Clutter actors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum AccessibleRole {
    #[default]
    Invalid = 0,
    Window = 1,
    Panel = 2,
    PushButton = 3,
    Text = 4,
    Image = 5,
    List = 6,
    ListItem = 7,
    ScrollBar = 8,
    Canvas = 9,
    Application = 10,
}

/// `AtkStateType` — common subset of ATK states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum AccessibleState {
    #[default]
    None = 0,
    Focused = 1,
    Visible = 2,
    Enabled = 3,
    Sensitive = 4,
    Showing = 5,
    Editable = 6,
    Busy = 7,
}

/// Placeholder for `AtkObject` — the root accessible object.
#[derive(Debug, Clone)]
pub struct AccessibilityRoot {
    id: u32,
    name: String,
    description: String,
    role: AccessibleRole,
    states: Vec<AccessibleState>,
    children: Vec<u32>,
}

impl AccessibilityRoot {
    pub fn new(id: u32) -> Self {
        AccessibilityRoot {
            id,
            name: String::from("Clutter Application"),
            description: String::from("Clutter accessibility root"),
            role: AccessibleRole::Application,
            states: vec![AccessibleState::Visible, AccessibleState::Enabled],
            children: Vec::new(),
        }
    }

    pub fn id(&self) -> u32 { self.id }
    pub fn name(&self) -> &str { &self.name }
    pub fn set_name(&mut self, name: impl Into<String>) { self.name = name.into(); }
    pub fn description(&self) -> &str { &self.description }
    pub fn set_description(&mut self, desc: impl Into<String>) { self.description = desc.into(); }
    pub fn role(&self) -> AccessibleRole { self.role }
    pub fn states(&self) -> &[AccessibleState] { &self.states }

    pub fn add_state(&mut self, state: AccessibleState) {
        if !self.states.contains(&state) { self.states.push(state); }
    }

    pub fn remove_state(&mut self, state: AccessibleState) {
        self.states.retain(|s| *s != state);
    }

    pub fn has_state(&self, state: AccessibleState) -> bool {
        self.states.contains(&state)
    }

    pub fn add_child(&mut self, child_id: u32) {
        if !self.children.contains(&child_id) { self.children.push(child_id); }
    }

    pub fn remove_child(&mut self, child_id: u32) {
        self.children.retain(|c| *c != child_id);
    }

    pub fn children(&self) -> &[u32] { &self.children }
    pub fn n_children(&self) -> usize { self.children.len() }
}

/// Port of the accessibility state from `clutter-accessibility.c`.
#[derive(Debug)]
pub struct AccessibilityState {
    enabled: bool,
    initialized: bool,
    root: Option<AccessibilityRoot>,
    next_id: u32,
}

impl Default for AccessibilityState {
    fn default() -> Self { Self::new() }
}

impl AccessibilityState {
    pub fn new() -> Self {
        AccessibilityState { enabled: true, initialized: false, root: None, next_id: 1 }
    }

    /// `clutter_accessibility_init`. Creates the root accessible object.
    /// Returns `false` if already initialized.
    pub fn init(&mut self) -> bool {
        if self.initialized { return false; }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.root = Some(AccessibilityRoot::new(id));
        self.initialized = true;
        true
    }

    /// `clutter_accessibility_get_default`. Returns the root, initializing on first access.
    pub fn get_root(&mut self) -> &AccessibilityRoot {
        if !self.initialized { self.init(); }
        self.root.as_ref().unwrap()
    }

    pub fn get_root_mut(&mut self) -> &mut AccessibilityRoot {
        if !self.initialized { self.init(); }
        self.root.as_mut().unwrap()
    }

    pub fn is_enabled(&self) -> bool { self.enabled }
    pub fn disable(&mut self) { self.enabled = false; }
    pub fn enable(&mut self) { self.enabled = true; }
    pub fn set_enabled(&mut self, enabled: bool) { self.enabled = enabled; }
    pub fn is_initialized(&self) -> bool { self.initialized }

    /// Allocates a new accessible object id.
    pub fn allocate_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_defaults() {
        let state = AccessibilityState::new();
        assert!(state.is_enabled());
        assert!(!state.is_initialized());
    }

    #[test]
    fn init_creates_root() {
        let mut state = AccessibilityState::new();
        assert!(state.init());
        assert!(state.is_initialized());
        assert!(!state.init()); // second init is no-op
    }

    #[test]
    fn get_root_initializes_lazily() {
        let mut state = AccessibilityState::new();
        assert!(!state.is_initialized());
        let _ = state.get_root();
        assert!(state.is_initialized());
    }

    #[test]
    fn disable_accessibility() {
        let mut state = AccessibilityState::new();
        assert!(state.is_enabled());
        state.disable();
        assert!(!state.is_enabled());
        state.enable();
        assert!(state.is_enabled());
    }

    #[test]
    fn root_has_default_states() {
        let mut state = AccessibilityState::new();
        let root = state.get_root();
        assert!(root.has_state(AccessibleState::Visible));
        assert!(root.has_state(AccessibleState::Enabled));
        assert_eq!(root.role(), AccessibleRole::Application);
    }

    #[test]
    fn root_add_remove_state() {
        let mut state = AccessibilityState::new();
        let root = state.get_root_mut();
        assert!(!root.has_state(AccessibleState::Focused));
        root.add_state(AccessibleState::Focused);
        assert!(root.has_state(AccessibleState::Focused));
        root.remove_state(AccessibleState::Focused);
        assert!(!root.has_state(AccessibleState::Focused));
    }

    #[test]
    fn root_add_remove_child() {
        let mut state = AccessibilityState::new();
        let root = state.get_root_mut();
        assert_eq!(root.n_children(), 0);
        root.add_child(1);
        root.add_child(2);
        assert_eq!(root.n_children(), 2);
        root.add_child(1); // duplicate no-op
        assert_eq!(root.n_children(), 2);
        root.remove_child(1);
        assert_eq!(root.n_children(), 1);
    }

    #[test]
    fn allocate_id_increments() {
        let mut state = AccessibilityState::new();
        assert_eq!(state.allocate_id(), 1);
        assert_eq!(state.allocate_id(), 2);
        assert_eq!(state.allocate_id(), 3);
    }
}
