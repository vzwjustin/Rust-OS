//! Wayland window actor ported from `meta-window-actor-wayland.c`.
//!
//! Specialized window actor for Wayland client windows.

use super::surface_actor_wayland::SurfaceActorWayland;
use super::window_actor::WindowActor;
use crate::desktop::window_manager::WindowId;

/// Wayland-specific window actor
#[derive(Debug)]
pub struct WindowActorWayland {
    pub base: WindowActor,
    pub wayland_surface: Option<SurfaceActorWayland>,
    pub pending_commit: bool,
}

impl WindowActorWayland {
    /// Create new Wayland window actor
    pub fn new(id: u32, window_id: WindowId) -> Self {
        WindowActorWayland {
            base: WindowActor::new(id, window_id),
            wayland_surface: None,
            pending_commit: false,
        }
    }

    /// Set Wayland surface actor
    pub fn set_surface_actor(&mut self, surface: SurfaceActorWayland) {
        self.wayland_surface = Some(surface);
    }

    /// Mark pending commit
    pub fn set_pending_commit(&mut self) {
        self.pending_commit = true;
    }

    /// Process pending commit
    pub fn commit(&mut self) -> bool {
        if self.pending_commit {
            self.pending_commit = false;
            true
        } else {
            false
        }
    }

    /// Handle buffer callback
    pub fn handle_buffer_attach(&mut self, buffer_id: u32) {
        if let Some(ref mut surface) = self.wayland_surface {
            surface.attach_buffer(buffer_id);
        }
    }

    /// Paint Wayland window
    pub fn paint(&self) {
        if let Some(ref surface) = self.wayland_surface {
            surface.paint();
        }
    }
}
