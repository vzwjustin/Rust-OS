//! Keybindings management ported from GNOME Mutter's src/core/keybindings.c
//!
//! Implements keyboard binding management for window manager operations like
//! workspace switching, window focusing, tiling, etc.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/keybindings.c

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Keyboard modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyModifier {
    /// Shift key.
    Shift,
    /// Control/Ctrl key.
    Control,
    /// Alt/Meta key.
    Alt,
    /// Super/Windows key.
    Super,
}

/// Virtual key codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyCode(pub u32);

/// Keyboard action type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    // Workspace actions
    /// Switch to previous workspace.
    SwitchWorkspacePrevious,
    /// Switch to next workspace.
    SwitchWorkspaceNext,
    /// Switch to workspace N.
    SwitchWorkspace(usize),
    /// Move window to previous workspace.
    MoveWindowToPreviousWorkspace,
    /// Move window to next workspace.
    MoveWindowToNextWorkspace,
    /// Move window to workspace N.
    MoveWindowToWorkspace(usize),

    // Window actions
    /// Close active window.
    CloseWindow,
    /// Minimize active window.
    MinimizeWindow,
    /// Maximize active window.
    MaximizeWindow,
    /// Unmaximize active window.
    UnmaximizeWindow,
    /// Toggle maximize.
    ToggleMaximize,
    /// Tile window left.
    TileWindowLeft,
    /// Tile window right.
    TileWindowRight,
    /// Toggle fullscreen.
    ToggleFullscreen,

    // Focus actions
    /// Focus next window.
    FocusNextWindow,
    /// Focus previous window.
    FocusPreviousWindow,
    /// Raise active window.
    RaiseWindow,
    /// Lower active window.
    LowerWindow,

    // Custom action.
    /// Custom user action.
    Custom(u32),
}

/// Represents a keyboard binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding {
    /// Key code.
    pub key: KeyCode,
    /// Required modifiers (all must be pressed).
    pub modifiers: Vec<KeyModifier>,
    /// Action triggered by this binding.
    pub action: KeyAction,
    /// Whether binding is enabled.
    pub enabled: bool,
}

impl KeyBinding {
    /// Create a new key binding.
    pub fn new(key: KeyCode, modifiers: Vec<KeyModifier>, action: KeyAction) -> Self {
        KeyBinding {
            key,
            modifiers,
            action,
            enabled: true,
        }
    }

    /// Check if binding requires a specific modifier.
    pub fn requires_modifier(&self, modifier: KeyModifier) -> bool {
        self.modifiers.contains(&modifier)
    }

    /// Enable this binding.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable this binding.
    pub fn disable(&mut self) {
        self.enabled = false;
    }
}

/// Manages keyboard bindings for window manager.
#[derive(Debug)]
pub struct KeyBindingManager {
    /// Map of (KeyCode, modifiers_mask) to KeyBinding.
    bindings: BTreeMap<(KeyCode, u32), KeyBinding>,
    /// Action to bindings index for quick lookup.
    action_index: BTreeMap<u32, Vec<(KeyCode, u32)>>,
    /// Whether key grabbing is active.
    keys_grabbed: bool,
}

impl KeyBindingManager {
    /// Create a new key binding manager.
    pub fn new() -> Self {
        KeyBindingManager {
            bindings: BTreeMap::new(),
            action_index: BTreeMap::new(),
            keys_grabbed: false,
        }
    }

    /// Register a new keybinding.
    pub fn register_binding(&mut self, binding: KeyBinding) {
        let mask = self.modifiers_to_mask(&binding.modifiers);
        let key = (binding.key, mask);

        self.bindings.insert(key, binding);
    }

    /// Unregister a keybinding by key and modifiers.
    pub fn unregister_binding(&mut self, key: KeyCode, modifiers: &[KeyModifier]) {
        let mask = self.modifiers_to_mask(modifiers);
        self.bindings.remove(&(key, mask));
    }

    /// Look up a keybinding by key and modifiers.
    pub fn lookup_binding(&self, key: KeyCode, modifiers: &[KeyModifier]) -> Option<&KeyBinding> {
        let mask = self.modifiers_to_mask(modifiers);
        self.bindings.get(&(key, mask))
    }

    /// Get all registered bindings.
    pub fn all_bindings(&self) -> Vec<&KeyBinding> {
        self.bindings.values().collect()
    }

    /// Get enabled bindings only.
    pub fn enabled_bindings(&self) -> Vec<&KeyBinding> {
        self.bindings.values().filter(|b| b.enabled).collect()
    }

    /// Handle a key press event.
    pub fn handle_key_press(&self, key: KeyCode, modifiers: &[KeyModifier]) -> Option<KeyAction> {
        if let Some(binding) = self.lookup_binding(key, modifiers) {
            if binding.enabled {
                return Some(binding.action);
            }
        }
        None
    }

    /// Enable all keybindings.
    pub fn enable_all(&mut self) {
        for binding in self.bindings.values_mut() {
            binding.enable();
        }
    }

    /// Disable all keybindings.
    pub fn disable_all(&mut self) {
        for binding in self.bindings.values_mut() {
            binding.disable();
        }
    }

    /// Enable/disable by action.
    pub fn set_action_enabled(&mut self, action: KeyAction, enabled: bool) {
        for binding in self.bindings.values_mut() {
            if binding.action == action {
                if enabled {
                    binding.enable();
                } else {
                    binding.disable();
                }
            }
        }
    }

    /// Mark keys as grabbed by window manager.
    pub fn set_keys_grabbed(&mut self, grabbed: bool) {
        self.keys_grabbed = grabbed;
    }

    /// Check if keys are grabbed.
    pub fn keys_grabbed(&self) -> bool {
        self.keys_grabbed
    }

    /// Convert modifier list to bitmask for quick comparison.
    fn modifiers_to_mask(&self, modifiers: &[KeyModifier]) -> u32 {
        modifiers
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, _)| acc | (1 << i))
    }
}

impl Default for KeyBindingManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_binding_creation() {
        let binding = KeyBinding::new(KeyCode(1), vec![KeyModifier::Alt], KeyAction::CloseWindow);

        assert_eq!(binding.key, KeyCode(1));
        assert!(binding.requires_modifier(KeyModifier::Alt));
        assert!(binding.enabled);
    }

    #[test]
    fn test_manager_register() {
        let mut mgr = KeyBindingManager::new();
        let binding = KeyBinding::new(KeyCode(1), vec![KeyModifier::Alt], KeyAction::CloseWindow);

        mgr.register_binding(binding);
        let found = mgr.lookup_binding(KeyCode(1), &[KeyModifier::Alt]);
        assert!(found.is_some());
    }

    #[test]
    fn test_key_press_handling() {
        let mut mgr = KeyBindingManager::new();
        let binding = KeyBinding::new(
            KeyCode(42),
            vec![KeyModifier::Alt, KeyModifier::Control],
            KeyAction::CloseWindow,
        );

        mgr.register_binding(binding);

        let action = mgr.handle_key_press(KeyCode(42), &[KeyModifier::Alt, KeyModifier::Control]);
        assert_eq!(action, Some(KeyAction::CloseWindow));
    }

    #[test]
    fn test_disable_binding() {
        let mut mgr = KeyBindingManager::new();
        let binding = KeyBinding::new(KeyCode(1), vec![KeyModifier::Alt], KeyAction::CloseWindow);

        mgr.register_binding(binding);
        mgr.set_action_enabled(KeyAction::CloseWindow, false);

        let action = mgr.handle_key_press(KeyCode(1), &[KeyModifier::Alt]);
        assert_eq!(action, None);
    }

    #[test]
    fn test_workspace_actions() {
        let mut mgr = KeyBindingManager::new();

        let binding1 = KeyBinding::new(
            KeyCode(1),
            vec![KeyModifier::Alt],
            KeyAction::SwitchWorkspaceNext,
        );
        let binding2 = KeyBinding::new(
            KeyCode(2),
            vec![KeyModifier::Alt],
            KeyAction::SwitchWorkspacePrevious,
        );

        mgr.register_binding(binding1);
        mgr.register_binding(binding2);

        let action1 = mgr.handle_key_press(KeyCode(1), &[KeyModifier::Alt]);
        assert_eq!(action1, Some(KeyAction::SwitchWorkspaceNext));

        let action2 = mgr.handle_key_press(KeyCode(2), &[KeyModifier::Alt]);
        assert_eq!(action2, Some(KeyAction::SwitchWorkspacePrevious));
    }
}
