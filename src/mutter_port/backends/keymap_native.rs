//! Keymap Native — ported from GNOME Mutter
//!
//! Native platform keymap handling for X11/Wayland input systems.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-keymap-native.h
//! Upstream header not found; minimal stub.

/// Opaque native keymap.
pub struct MetaKeymapNative;

impl MetaKeymapNative {
    /// Create a new native keymap.
    pub fn new() -> Self {
        MetaKeymapNative
    }
}

impl Default for MetaKeymapNative {
    fn default() -> Self {
        Self::new()
    }
}