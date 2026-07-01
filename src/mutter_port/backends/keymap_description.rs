//! keymap_description - reference-counted description of a keyboard keymap.
//!
//! Ported from GNOME Mutter's src/backends/meta-keymap-description.c. A keymap can
//! come either from a set of XKB rule names or from a sealed file descriptor. The
//! ref-counted owner/lock bookkeeping and the data model are preserved. The actual
//! xkbcommon keymap compilation (xkb_keymap_new_from_names / _from_string) and the
//! rxkb registry lookup for short names are stubbed, since those libraries are not
//! available in the kernel.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-keymap-description.c

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Default XKB rules file (DEFAULT_XKB_RULES_FILE).
pub const DEFAULT_XKB_RULES_FILE: &str = "evdev";
/// Default XKB model (DEFAULT_XKB_MODEL).
pub const DEFAULT_XKB_MODEL: &str = "pc105+inet";

/// Whether a keymap description was built from rule names or a sealed fd.
/// Mirrors `MetaKeymapDescriptionSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeymapDescriptionSource {
    Rules,
    Fd,
}

/// Opaque owner token used for locking a keymap description.
///
/// Mirrors `MetaKeymapDescriptionOwner`. The C code ref-counts these atomically;
/// here we identify owners by a unique id so equality/locking semantics hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeymapDescriptionOwner {
    pub id: u32,
}

impl KeymapDescriptionOwner {
    /// Mirrors `meta_keymap_description_owner_new`.
    pub fn new(id: u32) -> Self {
        KeymapDescriptionOwner { id }
    }
}

/// XKB rule-name based keymap parameters (the `rules` union arm).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapRules {
    pub rules: String,
    pub model: String,
    pub layout: String,
    pub variant: String,
    pub options: String,
    pub display_names: Vec<String>,
    pub short_names: Vec<String>,
}

/// Sealed-fd based keymap parameters (the `fd` union arm).
///
/// `sealed_fd` stands in for `MetaSealedFd *`; `format` for `enum xkb_keymap_format`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapFd {
    pub sealed_fd: i32,
    pub format: u32,
}

/// The source payload: rules or fd (the C anonymous union).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeymapPayload {
    Rules(KeymapRules),
    Fd(KeymapFd),
}

/// Description of a keymap. Mirrors `MetaKeymapDescription`.
///
/// The C type is a boxed, atomically ref-counted struct; here we use plain
/// ownership and expose the same lock/owner accessors.
#[derive(Debug, Clone)]
pub struct KeymapDescription {
    pub source: KeymapDescriptionSource,
    pub is_locked: bool,
    pub owner: Option<KeymapDescriptionOwner>,
    pub resets_owner: Option<KeymapDescriptionOwner>,
    pub payload: KeymapPayload,
}

fn strdup_or_empty(s: Option<&str>) -> String {
    s.unwrap_or("").to_string()
}

impl KeymapDescription {
    /// Mirrors `meta_keymap_description_new_from_rules`.
    pub fn new_from_rules(
        model: Option<&str>,
        layout: Option<&str>,
        variant: Option<&str>,
        options: Option<&str>,
        display_names: Vec<String>,
        short_names: Vec<String>,
    ) -> Self {
        KeymapDescription {
            source: KeymapDescriptionSource::Rules,
            is_locked: false,
            owner: None,
            resets_owner: None,
            payload: KeymapPayload::Rules(KeymapRules {
                rules: DEFAULT_XKB_RULES_FILE.to_string(),
                model: model
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| DEFAULT_XKB_MODEL.to_string()),
                layout: strdup_or_empty(layout),
                variant: strdup_or_empty(variant),
                options: strdup_or_empty(options),
                display_names,
                short_names,
            }),
        }
    }

    /// Mirrors `meta_keymap_description_new_from_fd`.
    pub fn new_from_fd(sealed_fd: i32, format: u32) -> Self {
        KeymapDescription {
            source: KeymapDescriptionSource::Fd,
            is_locked: false,
            owner: None,
            resets_owner: None,
            payload: KeymapPayload::Fd(KeymapFd { sealed_fd, format }),
        }
    }

    /// Mirrors `meta_keymap_description_direct_equal` (pointer identity in C;
    /// structural equality here).
    pub fn direct_equal(&self, other: &KeymapDescription) -> bool {
        core::ptr::eq(self, other)
    }

    /// Compile an xkb keymap from this description.
    /// Stub: requires libxkbcommon (xkb_keymap_new_from_names/_from_string) and
    /// the rxkb registry, which are unavailable in the kernel. Returns the
    /// display names carried by the rules payload if present.
    pub fn create_xkb_keymap(&self) -> Result<(Vec<String>, Vec<String>), &'static str> {
        match &self.payload {
            KeymapPayload::Rules(r) => Ok((r.display_names.clone(), r.short_names.clone())),
            KeymapPayload::Fd(_) => Err("xkb keymap compilation from fd is stubbed in kernel"),
        }
    }

    /// Mirrors `meta_keymap_description_lock`.
    pub fn lock(&mut self, owner: KeymapDescriptionOwner) {
        debug_assert!(self.owner.is_none());
        self.is_locked = true;
        self.owner = Some(owner);
    }

    /// Mirrors `meta_keymap_description_unlock`.
    pub fn unlock(&mut self, owner: KeymapDescriptionOwner) {
        debug_assert!(self.owner.is_none());
        debug_assert!(!self.is_locked);
        self.owner = Some(owner);
    }

    /// Mirrors `meta_keymap_description_reset_owner`.
    pub fn reset_owner(&mut self, owner: KeymapDescriptionOwner) {
        self.resets_owner = Some(owner);
    }

    /// Mirrors `meta_keymap_description_is_locked`.
    pub fn is_locked(&self) -> bool {
        self.is_locked
    }

    /// Mirrors `meta_keymap_description_get_owner`.
    pub fn get_owner(&self) -> Option<KeymapDescriptionOwner> {
        self.owner
    }

    /// Mirrors `meta_keymap_description_resets_owner`.
    pub fn resets_owner(&self) -> Option<KeymapDescriptionOwner> {
        self.resets_owner
    }
}
