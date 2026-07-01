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
    // TODO: add all keybinding actions
}

/// Represents a keybinding
pub struct MetaKeyBinding {
    // TODO: port keybinding fields
    pub name: String,
    pub action: MetaKeybindingAction,
}

impl MetaKeyBinding {
    pub fn new(name: String, action: MetaKeybindingAction) -> Self {
        Self { name, action }
    }

    /// Get binding name
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Get binding action
    pub fn get_action(&self) -> MetaKeybindingAction {
        self.action
    }
}

/// Keyboard layout/keymap information
pub struct MetaKeymapDescription {
    current_layout: Option<String>,
    available_layouts: Vec<String>,
}

impl MetaKeymapDescription {
    /// Create a new MetaKeymapDescription
    pub fn new() -> Self {
        Self {
            current_layout: None,
            available_layouts: Vec::new(),
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

    /// Switch keyboard layout
    pub fn set_layout(&mut self, layout: &str) {
        self.current_layout = Some(String::from(layout));
        // TODO: implement
    }
}

impl Default for MetaKeymapDescription {
    fn default() -> Self {
        Self::new()
    }
}
