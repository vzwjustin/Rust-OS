//! Mutter cursor tracking
//! Ported from meta/meta-cursor-tracker.h

use crate::mutter_port::meta::types::*;
use alloc::string::String;

/// Tracks cursor position and visibility
pub struct MetaCursorTracker {
    pub x: i32,
    pub y: i32,
    visible: bool,
    cursor_name: Option<String>,
    theme: Option<String>,
}

impl MetaCursorTracker {
    /// Create a new cursor tracker
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            visible: true,
            cursor_name: None,
            theme: None,
        }
    }

    /// Get current cursor position
    pub fn get_position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// Set cursor position
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
        // TODO: implement
    }

    /// Show cursor
    pub fn show_cursor(&mut self) {
        self.visible = true;
    }

    /// Hide cursor
    pub fn hide_cursor(&mut self) {
        self.visible = false;
    }

    /// Check if cursor is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Set cursor theme
    pub fn set_theme(&mut self, theme: &str) {
        self.theme = Some(String::from(theme));
    }

    /// Get cursor theme
    pub fn get_theme(&self) -> Option<&str> {
        self.theme.as_ref().map(|s| s.as_str())
    }

    /// Get cursor sprite
    pub fn get_cursor(&self) -> Option<&str> {
        // TODO: implement
        self.cursor_name.as_ref().map(|s| s.as_str())
    }
}

impl Default for MetaCursorTracker {
    fn default() -> Self {
        Self::new()
    }
}
