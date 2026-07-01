//! Keymap Description Private — ported from GNOME Mutter
//!
//! Manages keymap description ownership and lifecycle, including locked/unlocked states
//! and FD-based keymap creation from xkbcommon format definitions.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-keymap-description-private.h

use alloc::vec::Vec;
use core::ffi::c_void;

/// XKB keymap format enum (from xkbcommon).
/// Represents the serialization format for keymaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum XkbKeymapFormat {
    /// Text format keymap.
    TEXT = 0,
    /// Binary format keymap.
    BINARY = 1,
}

/// Source of keymap description (rules file or file descriptor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKeymapDescriptionSource {
    META_KEYMAP_DESCRIPTION_SOURCE_RULES = 0,
    META_KEYMAP_DESCRIPTION_SOURCE_FD = 1,
}

/// Opaque keymap description owner handle.
///
/// Manages exclusive access to keymap descriptions during modifications.
/// Reference-counted handle for synchronization.
pub struct MetaKeymapDescriptionOwner {
    /// Reference count for this owner (opaque lifecycle management).
    _ref_count: *mut c_void,
}

impl MetaKeymapDescriptionOwner {
    /// Create a new keymap description owner.
    pub fn new() -> Self {
        MetaKeymapDescriptionOwner {
            _ref_count: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaKeymapDescriptionOwner {
    fn default() -> Self {
        Self::new()
    }
}

/// Keymap description — encapsulates xkb keymap layout and ownership.
///
/// Holds either a rules-based keymap or an FD-based (pre-compiled) keymap,
/// with optional owner lock state for synchronization.
pub struct MetaKeymapDescription {
    /// Source type: rules file or file descriptor.
    pub source: MetaKeymapDescriptionSource,
    /// XKB keymap format (TEXT or BINARY).
    pub format: XkbKeymapFormat,
    /// Sealed file descriptor for FD-based keymaps (opaque).
    pub sealed_fd: *mut c_void,
    /// Current owner (if locked), null if unlocked.
    pub owner: *mut MetaKeymapDescriptionOwner,
    /// Owner that resets lock state on drop.
    pub reset_owner: *mut MetaKeymapDescriptionOwner,
    /// Cached xkb keymap (opaque).
    pub xkb_keymap: *mut c_void,
}

impl MetaKeymapDescription {
    /// Create a new keymap description with default state.
    pub fn new() -> Self {
        MetaKeymapDescription {
            source: MetaKeymapDescriptionSource::META_KEYMAP_DESCRIPTION_SOURCE_RULES,
            format: XkbKeymapFormat::TEXT,
            sealed_fd: core::ptr::null_mut(),
            owner: core::ptr::null_mut(),
            reset_owner: core::ptr::null_mut(),
            xkb_keymap: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaKeymapDescription {
    fn default() -> Self {
        Self::new()
    }
}
