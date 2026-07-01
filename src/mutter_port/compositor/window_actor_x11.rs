//! X11 window actor (stub) ported from `meta-window-actor-x11.c`.
//!
//! X11 support is explicitly out of scope for RustOS (Wayland-first design).
//! This module is provided as a minimal stub for completeness.
//! X11 window rendering is not implemented; use Wayland-based windows instead.

use super::window_actor::WindowActor;
use crate::desktop::window_manager::WindowId;

/// X11 window actor (not implemented - Wayland only)
#[derive(Debug)]
pub struct WindowActorX11 {
    pub base: WindowActor,
    /// X11 window ID
    pub x11_window: u32,
}

impl WindowActorX11 {
    /// Create new X11 window actor
    /// Note: X11 support is out of scope for RustOS
    pub fn new(id: u32, window_id: WindowId, x11_window: u32) -> Self {
        WindowActorX11 {
            base: WindowActor::new(id, window_id),
            x11_window,
        }
    }

    /// X11 rendering not implemented
    pub fn paint(&self) {
        // X11 legacy protocol not supported on RustOS (Wayland-first)
    }
}
