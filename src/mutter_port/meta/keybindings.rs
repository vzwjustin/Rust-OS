//! Mutter keybinding management
//! Ported from meta/keybindings.h and meta/meta-keymap-description.h
//!
//! MetaKeyBinding maps keyboard input to window manager actions.
//! MetaKeymapDescription manages keyboard layout and input method state.
use alloc::{string::String, vec::Vec};

use crate::mutter_port::meta::types::*;

/// Keybinding action types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaKeybindingAction {
    WindowClose = 0,
    WindowMinimize = 1,
    WindowMaximize = 2,
    WindowMaximizeHorizontally = 3,
    WindowMaximizeVertically = 4,
    WindowMove = 5,
    WindowResize = 6,
    WindowToggleMaximized = 7,
    WindowUnmaximize = 8,
    WindowShade = 9,
    WindowUnshade = 10,
    WindowRaise = 11,
    WindowLower = 12,
    WindowFullscreen = 13,
    WindowUnfullscreen = 14,
    WindowAbove = 15,
    WindowBelow = 16,
    WindowTileLeft = 17,
    WindowTileRight = 18,
    WindowTileUp = 19,
    WindowTileDown = 20,
    WorkspaceSwitchLeft = 21,
    WorkspaceSwitchRight = 22,
    WorkspaceSwitchUp = 23,
    WorkspaceSwitchDown = 24,
    WorkspaceToggleOnAllWorkspaces = 25,
    PanelRunDialog = 26,
    PanelMainMenu = 27,
    ToggleRecording = 28,
    SwitchApplications = 29,
    SwitchWindows = 30,
    SwitchWindowsBackward = 31,
    SwitchApplicationsBackward = 32,
    CycleWindows = 33,
    CycleWindowsBackward = 34,
    CyclePanels = 35,
    ShowDesktop = 36,
    SetSpewMark = 37,
    None = 999,
}

/// Represents a keybinding
pub struct MetaKeyBinding {
    pub name: String,
    pub action: MetaKeybindingAction,
    /// Keycode that triggers this binding.
    pub keycode: u32,
    /// Modifier mask (Shift, Ctrl, Alt, etc.).
    pub modifiers: u32,
    /// Whether this is a builtin (vs. custom) binding.
    pub is_builtin: bool,
}

impl MetaKeyBinding {
    pub fn new(name: String, action: MetaKeybindingAction) -> Self {
        Self {
            name,
            action,
            keycode: 0,
            modifiers: 0,
            is_builtin: true,
        }
    }

    /// Get binding name
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Get binding action
    pub fn get_action(&self) -> MetaKeybindingAction {
        self.action
    }

    /// Get the keycode.
    pub fn get_keycode(&self) -> u32 {
        self.keycode
    }

    /// Get the modifier mask.
    pub fn get_modifiers(&self) -> u32 {
        self.modifiers
    }

    /// Set the keycode and modifiers that trigger this binding.
    pub fn set_keys(&mut self, keycode: u32, modifiers: u32) {
        self.keycode = keycode;
        self.modifiers = modifiers;
    }

    /// Check if a key event matches this binding.
    pub fn matches(&self, keycode: u32, modifiers: u32) -> bool {
        self.keycode == keycode && self.modifiers == modifiers
    }
}

/// Keyboard layout/keymap information
pub struct MetaKeymapDescription {
    current_layout: Option<String>,
    available_layouts: Vec<String>,
    /// Whether the layout has changed and needs to be applied to the
    /// XKB state (dirty flag).
    layout_dirty: bool,
}

impl MetaKeymapDescription {
    /// Create a new MetaKeymapDescription
    pub fn new() -> Self {
        Self {
            current_layout: None,
            available_layouts: Vec::new(),
            layout_dirty: false,
        }
    }

    /// Get current keyboard layout name
    pub fn get_layout(&self) -> Option<&str> {
        self.current_layout.as_ref().map(|s| s.as_str())
    }

    /// Get available layouts
    pub fn get_layouts(&self) -> Vec<String> {
        self.available_layouts.clone()
    }

    /// Whether the layout has changed and needs to be applied.
    pub fn is_layout_dirty(&self) -> bool {
        self.layout_dirty
    }

    /// Clear the dirty flag after the layout has been applied to XKB.
    pub fn clear_layout_dirty(&mut self) {
        self.layout_dirty = false;
    }

    /// Switch keyboard layout. Sets the current layout and marks it
    /// as dirty so the XKB state can be updated on the next dispatch.
    pub fn set_layout(&mut self, layout: &str) {
        self.current_layout = Some(String::from(layout));
        self.layout_dirty = true;
    }
}

impl Default for MetaKeymapDescription {
    fn default() -> Self {
        Self::new()
    }
}
