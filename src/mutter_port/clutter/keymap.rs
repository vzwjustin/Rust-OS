//! Port of GNOME mutter's `clutter/clutter-keymap.{c,h}`.
//!
//! Keymap interface managing keyboard state: lock states (caps/num lock),
//! modifier masks (depressed/latched/locked), keyboard layout switching,
//! and text direction. Subclasses implement `get_direction` to determine
//! layout-specific directionality (LTR/RTL).

pub type XkbModMask = u32;
pub type XkbLayoutIndex = u32;

/// Text direction determined by current keyboard layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDirection {
    Ltr,
    Rtl,
}

/// Port of `ClutterKeymapClass` vtable. Implement this per keymap source
/// (X11, Wayland, etc.) instead of subclassing the GObject.
///
/// The `get_direction` virtual is mandatory; other accessors have defaults
/// that return neutral/empty values. Concrete implementations query their
/// backend's keyboard state and override accessors as needed.
pub trait Keymap {
    /// `ClutterKeymapClass::get_direction`: return layout-specific text
    /// direction (LTR or RTL).
    fn get_direction(&self) -> TextDirection;

    /// `clutter_keymap_get_caps_lock_state`: return whether Caps Lock is
    /// active. Default: false.
    fn caps_lock_state(&self) -> bool {
        false
    }

    /// `clutter_keymap_get_num_lock_state`: return whether Num Lock is
    /// active. Default: false.
    fn num_lock_state(&self) -> bool {
        false
    }

    /// `clutter_keymap_get_modifier_state`: return depressed, latched,
    /// locked modifier masks (xkbcommon style). Default: (0, 0, 0).
    fn modifier_state(&self) -> (XkbModMask, XkbModMask, XkbModMask) {
        (0, 0, 0)
    }

    /// `clutter_keymap_get_layout_index`: return the active keyboard layout
    /// group index. Default: 0.
    fn layout_index(&self) -> XkbLayoutIndex {
        0
    }

    /// `clutter_keymap_get_current_display_name`: return the localized
    /// display name of the current layout (e.g. "English" or "Ελληνικά").
    /// Default: None.
    fn current_display_name(&self) -> Option<&str> {
        None
    }

    /// `clutter_keymap_get_current_short_name`: return the short code of
    /// the current layout (e.g. "us" or "el"). Default: None.
    fn current_short_name(&self) -> Option<&str> {
        None
    }
}

// ---- wrapper functions matching the C `clutter_keymap_*` API ----

/// `clutter_keymap_get_direction`.
pub fn get_direction<K: Keymap + ?Sized>(keymap: &K) -> TextDirection {
    keymap.get_direction()
}

/// `clutter_keymap_get_caps_lock_state`.
pub fn caps_lock_state<K: Keymap + ?Sized>(keymap: &K) -> bool {
    keymap.caps_lock_state()
}

/// `clutter_keymap_get_num_lock_state`.
pub fn num_lock_state<K: Keymap + ?Sized>(keymap: &K) -> bool {
    keymap.num_lock_state()
}

/// `clutter_keymap_get_modifier_state`.
pub fn modifier_state<K: Keymap + ?Sized>(keymap: &K) -> (XkbModMask, XkbModMask, XkbModMask) {
    keymap.modifier_state()
}

/// `clutter_keymap_get_layout_index`.
pub fn layout_index<K: Keymap + ?Sized>(keymap: &K) -> XkbLayoutIndex {
    keymap.layout_index()
}

/// `clutter_keymap_get_current_display_name`.
pub fn current_display_name<K: Keymap + ?Sized>(keymap: &K) -> Option<&str> {
    keymap.current_display_name()
}

/// `clutter_keymap_get_current_short_name`.
pub fn current_short_name<K: Keymap + ?Sized>(keymap: &K) -> Option<&str> {
    keymap.current_short_name()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestKeymap {
        caps_locked: bool,
        num_locked: bool,
    }

    impl Keymap for TestKeymap {
        fn get_direction(&self) -> TextDirection {
            TextDirection::Ltr
        }
        fn caps_lock_state(&self) -> bool {
            self.caps_locked
        }
        fn num_lock_state(&self) -> bool {
            self.num_locked
        }
    }

    #[test]
    fn get_direction_returns_impl_value() {
        let km = TestKeymap {
            caps_locked: false,
            num_locked: false,
        };
        assert_eq!(get_direction(&km), TextDirection::Ltr);
    }

    #[test]
    fn lock_states_return_impl_values() {
        let km = TestKeymap {
            caps_locked: true,
            num_locked: false,
        };
        assert_eq!(caps_lock_state(&km), true);
        assert_eq!(num_lock_state(&km), false);
    }

    #[test]
    fn default_impl_returns_none() {
        let km = TestKeymap {
            caps_locked: false,
            num_locked: false,
        };
        assert_eq!(current_display_name(&km), None);
        assert_eq!(current_short_name(&km), None);
        assert_eq!(modifier_state(&km), (0, 0, 0));
        assert_eq!(layout_index(&km), 0);
    }
}
