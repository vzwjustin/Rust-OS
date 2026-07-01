//! Keymap Description Private — ported from GNOME Mutter
//!
//! Manages keymap description ownership and lifecycle, including locked/unlocked states
//! and FD-based keymap creation from xkbcommon format definitions.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-keymap-description-private.h

use alloc::vec::Vec;

/// Source of keymap description (rules file or file descriptor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKeymapDescriptionSource {
    META_KEYMAP_DESCRIPTION_SOURCE_RULES = 0,
    META_KEYMAP_DESCRIPTION_SOURCE_FD = 1,
}

/// Opaque keymap description owner handle.
/// Manages exclusive access to keymap descriptions during modifications.
pub struct MetaKeymapDescriptionOwner;

impl MetaKeymapDescriptionOwner {
    /// Create a new keymap description owner.
    pub fn new() -> Self {
        MetaKeymapDescriptionOwner
    }
}

impl Default for MetaKeymapDescriptionOwner {
    fn default() -> Self {
        Self::new()
    }
}

/// Keymap description — encapsulates xkb keymap layout and ownership.
pub struct MetaKeymapDescription {
    // TODO: Port fields from upstream C struct
}

impl MetaKeymapDescription {
    /// Create a new keymap description (stub).
    pub fn new() -> Self {
        MetaKeymapDescription {}
    }
}

impl Default for MetaKeymapDescription {
    fn default() -> Self {
        Self::new()
    }
}