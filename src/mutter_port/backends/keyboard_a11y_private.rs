//! Keyboard A11Y Private
//!
//! Native Rust implementation (no direct Mutter C counterpart).
//! Internal private state for keyboard accessibility implementation.

use alloc::string::String;

/// Keyboard Accessibility Private State — internal implementation details.
/// Holds private state and timers for keyboard accessibility features.
#[derive(Debug, Clone)]
pub struct KeyboardA11YPrivate {
    pub toggle_slowkeys_timer: u32,
    pub slowkeys_delay: u32,
}

impl KeyboardA11YPrivate {
    /// Create private a11y state.
    pub fn new() -> Self {
        KeyboardA11YPrivate {
            toggle_slowkeys_timer: 0,
            slowkeys_delay: 0,
        }
    }
}

impl Default for KeyboardA11YPrivate {
    fn default() -> Self {
        Self::new()
    }
}
