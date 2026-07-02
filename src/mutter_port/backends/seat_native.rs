//! Seat Native — ported from GNOME Mutter
//!
//! Manages a native seat for input device handling and keyboard layout configuration.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-seat-native.h

use alloc::string::String;

/// Native seat for input device handling.
pub struct MetaSeatNative;

impl MetaSeatNative {
    pub fn new() -> Self {
        MetaSeatNative
    }
}

impl Default for MetaSeatNative {
    fn default() -> Self {
        Self::new()
    }
}
