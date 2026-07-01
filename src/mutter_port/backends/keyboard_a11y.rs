//! Keyboard A11Y
//!
//! Native Rust implementation (no direct Mutter C counterpart).
//! Manages keyboard accessibility features like sticky keys, slow keys, etc.

use alloc::string::String;

/// Keyboard Accessibility Manager — handles keyboard accessibility features.
/// Manages sticky keys, slow keys, bounce keys, and other keyboard a11y settings.
#[derive(Debug, Clone)]
pub struct KeyboardA11Y {
    pub enabled: bool,
    pub slow_keys_enabled: bool,
    pub sticky_keys_enabled: bool,
}

impl KeyboardA11Y {
    /// Create a new keyboard a11y manager.
    pub fn new() -> Self {
        KeyboardA11Y {
            enabled: false,
            slow_keys_enabled: false,
            sticky_keys_enabled: false,
        }
    }
}

impl Default for KeyboardA11Y {
    fn default() -> Self {
        Self::new()
    }
}
