//! Screen Cast Stream — ported from GNOME Mutter
//!
//! Individual video/audio stream within a screen cast session, handling frame capture
//! and encoding for transmission to remote clients.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-screen-cast-stream.h

use alloc::string::String;

// Re-export the screen cast flag constants from the parent module for stream
// flag tracking. (`screen_cast` exposes these as `u32` consts, not a named type.)
pub use super::screen_cast::{
    META_SCREEN_CAST_FLAG_IS_PLATFORM, META_SCREEN_CAST_FLAG_IS_RECORDING,
    META_SCREEN_CAST_FLAG_NONE,
};

/// A single stream (video or audio) within a screen cast session.
pub struct MetaScreenCastStream {
    flags: u32,
    /// Whether the stream is currently active (capturing).
    pub active: bool,
    /// Number of frames captured.
    pub frame_count: u64,
    /// Stream parameters (width, height, format — opaque).
    pub parameters: Option<String>,
}

impl MetaScreenCastStream {
    pub fn new(flags: u32) -> Self {
        MetaScreenCastStream {
            flags,
            active: false,
            frame_count: 0,
            parameters: None,
        }
    }

    pub fn get_flags(&self) -> u32 {
        self.flags
    }

    pub fn set_flags(&mut self, flags: u32) {
        self.flags = flags;
    }

    /// Start the stream capture.
    pub fn start(&mut self) {
        self.active = true;
    }

    /// Stop the stream capture.
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Whether the stream is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Record a captured frame. Increments the frame counter.
    pub fn record_frame(&mut self) {
        if self.active {
            self.frame_count += 1;
        }
    }

    /// Get the total number of frames captured.
    pub fn get_frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Set stream parameters (e.g., resolution, format).
    pub fn set_parameters(&mut self, params: String) {
        self.parameters = Some(params);
    }
}

impl Default for MetaScreenCastStream {
    fn default() -> Self {
        Self::new(META_SCREEN_CAST_FLAG_NONE)
    }
}
