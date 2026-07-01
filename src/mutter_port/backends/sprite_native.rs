//! Sprite Native — ported from GNOME Mutter
//!
//! Native rendering sprite for Clutter.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-sprite-native.h

/// Native sprite for rendering.
pub struct MetaSpriteNative;

impl MetaSpriteNative {
    pub fn new() -> Self {
        MetaSpriteNative
    }
}

impl Default for MetaSpriteNative {
    fn default() -> Self {
        Self::new()
    }
}