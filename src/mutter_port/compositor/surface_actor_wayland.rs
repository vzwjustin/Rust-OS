//! Wayland surface actor ported from `meta-surface-actor-wayland.c`.
//!
//! Handles rendering of Wayland client window surfaces.

use super::surface_actor::SurfaceActor;

/// Wayland-specific surface actor
#[derive(Debug)]
pub struct SurfaceActorWayland {
    pub base: SurfaceActor,
    pub wl_surface_id: u32,     // Wayland surface resource ID
    pub buffer_id: Option<u32>, // Associated buffer
    pub sync_request: bool,
}

impl SurfaceActorWayland {
    /// Create new Wayland surface actor
    pub fn new(id: u32, wl_surface_id: u32) -> Self {
        SurfaceActorWayland {
            base: SurfaceActor::new(id),
            wl_surface_id,
            buffer_id: None,
            sync_request: false,
        }
    }

    /// Attach Wayland buffer
    pub fn attach_buffer(&mut self, buffer_id: u32) {
        self.buffer_id = Some(buffer_id);
    }

    /// Detach current buffer
    pub fn detach_buffer(&mut self) {
        self.buffer_id = None;
    }

    /// Request frame synchronization
    pub fn request_frame_sync(&mut self) {
        self.sync_request = true;
    }

    /// Process frame callback
    pub fn frame_callback_done(&mut self) -> bool {
        if self.sync_request {
            self.sync_request = false;
            true
        } else {
            false
        }
    }

    /// Paint Wayland surface
    pub fn paint(&self) {
        if !self.base.visible || self.base.opacity == 0.0 {
            return;
        }

        // Render Wayland surface buffer at (x, y)
        if self.buffer_id.is_some() {
            // Paint buffer content
        }
    }
}
