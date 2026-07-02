//! Keymap Native Private — ported from GNOME Mutter
//!
//! Private platform-specific keymap implementation details for native backends.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-keymap-native-private.h
//! Upstream header not found; minimal stub.

/// Opaque native keymap private state.
pub struct MetaKeymapNativePrivate;

impl MetaKeymapNativePrivate {
    /// Create a new native keymap private state.
    pub fn new() -> Self {
        MetaKeymapNativePrivate
    }
}

impl Default for MetaKeymapNativePrivate {
    fn default() -> Self {
        Self::new()
    }
}
