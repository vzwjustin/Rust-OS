//! Mutter cursor tracking
//! Ported from meta/meta-cursor-tracker.h

use crate::mutter_port::meta::types::*;

/// Tracks cursor position and visibility
pub struct MetaCursorTracker {
    // TODO: port cursor tracker fields
    pub x: i32,
    pub y: i32,
}

impl MetaCursorTracker {
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
        // TODO: implement
    }

    /// Hide cursor
    pub fn hide_cursor(&mut self) {
        // TODO: implement
    }

    /// Check if cursor is visible
    pub fn is_visible(&self) -> bool {
        // TODO: implement
        true
    }

    /// Set cursor theme
    pub fn set_theme(&mut self, _theme: &str) {
        // TODO: implement
    }

    /// Get cursor sprite
    pub fn get_cursor(&self) -> Option<&str> {
        // TODO: implement
        None
    }
}

// TODO: port remaining cursor functions
