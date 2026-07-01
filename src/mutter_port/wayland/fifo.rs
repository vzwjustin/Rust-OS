//! Wayland FIFO — manages FIFO presentation queue support.
//!
//! Handles FIFO (First In, First Out) swap chain mode presentation for
//! Wayland clients. Coordinates with DRM to queue frames in FIFO order.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-fifo.h

/// FIFO presentation queue state for a surface.
///
/// In the C original, `MetaWaylandFifo` tracks whether FIFO mode is
/// enabled for a surface and how many frames are queued in the swap
/// chain. When FIFO is enabled, the compositor holds back commits
/// until the previous frame has been presented, ensuring in-order
/// delivery. A full implementation would coordinate with the DRM
/// backend's page-flip completion events to release queued frames.
#[derive(Debug)]
pub struct MetaWaylandFifo {
    /// Whether FIFO presentation mode is enabled for this surface.
    pub enabled: bool,
    /// Number of frames currently queued in the FIFO swap chain.
    pub queue_depth: u32,
}

impl MetaWaylandFifo {
    /// Create a new FIFO state with FIFO disabled and empty queue.
    pub fn new() -> Self {
        MetaWaylandFifo {
            enabled: false,
            queue_depth: 0,
        }
    }

    /// Enable FIFO presentation mode for the surface.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable FIFO presentation mode for the surface.
    /// Clears the queue depth since frames are no longer held.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.queue_depth = 0;
    }

    /// Check whether FIFO mode is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the current queue depth (number of queued frames).
    pub fn get_queue_depth(&self) -> u32 {
        self.queue_depth
    }

    /// Enqueue a frame in the FIFO swap chain.
    /// A full implementation would hold the surface commit until the
    /// previous frame's page-flip completes.
    pub fn enqueue_frame(&mut self) {
        if self.enabled {
            self.queue_depth = self.queue_depth.saturating_add(1);
        }
    }

    /// Dequeue a frame from the FIFO swap chain (called when a
    /// page-flip completes). Returns the new queue depth.
    /// A full implementation would release the next held commit.
    pub fn dequeue_frame(&mut self) -> u32 {
        if self.queue_depth > 0 {
            self.queue_depth -= 1;
        }
        self.queue_depth
    }

    /// Whether there are frames waiting in the queue.
    pub fn has_queued_frames(&self) -> bool {
        self.queue_depth > 0
    }
}

impl Default for MetaWaylandFifo {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize FIFO swap chain support for the compositor.
///
/// Sets up FIFO presentation mode handling. A full implementation would
/// register the wp_fifo_v1 protocol global and hook into the DRM
/// backend's page-flip completion callbacks.
pub fn meta_wayland_fifo_init(compositor: *mut core::ffi::c_void) {
    if compositor.is_null() {
        return;
    }
    // Protocol global registration requires libwayland-server.
}
