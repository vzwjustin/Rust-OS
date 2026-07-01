//! Screen Cast Stream — ported from GNOME Mutter
//!
//! Individual video/audio stream within a screen cast session, handling frame capture
//! and encoding for transmission to remote clients.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-screen-cast-stream.h

use alloc::string::String;

// Re-export the screen cast flag constants from the parent module for stream
// flag tracking. (`screen_cast` exposes these as `u32` consts, not a named type.)
pub use super::screen_cast::{META_SCREEN_CAST_FLAG_NONE,
                             META_SCREEN_CAST_FLAG_IS_RECORDING,
                             META_SCREEN_CAST_FLAG_IS_PLATFORM};

/// A single stream (video or audio) within a screen cast session.
pub struct MetaScreenCastStream {
    flags: u32,
    // TODO: Session reference, connection, stream state from C implementation
}

impl MetaScreenCastStream {
    pub fn new(flags: u32) -> Self {
        MetaScreenCastStream { flags }
    }

    pub fn get_flags(&self) -> u32 {
        self.flags
    }

    pub fn set_flags(&mut self, flags: u32) {
        self.flags = flags;
    }
}

impl Default for MetaScreenCastStream {
    fn default() -> Self {
        Self::new(META_SCREEN_CAST_FLAG_NONE)
    }
}