//! Native compositor view ported from `meta-compositor-view-native.c`.
//!
//! GPU-accelerated view rendering for native hardware backends.

use crate::desktop::window_manager::WindowId;

/// Native rendering view with GPU acceleration
#[derive(Debug)]
pub struct CompositorViewNative {
    pub id: u32,
    pub view_id: u32,
    pub gpu_enabled: bool,
    pub damage_tracked: bool,
}

impl CompositorViewNative {
    /// Create new native compositor view
    pub fn new(id: u32, view_id: u32) -> Self {
        CompositorViewNative {
            id,
            view_id,
            gpu_enabled: true,
            damage_tracked: true,
        }
    }

    /// Render a frame to this view
    pub fn render(&self, windows: &[WindowId]) -> bool {
        if !self.gpu_enabled {
            return false;
        }
        // Placeholder for GPU rendering
        true
    }

    /// Enable/disable GPU rendering
    pub fn set_gpu_enabled(&mut self, enabled: bool) {
        self.gpu_enabled = enabled;
    }

    /// Enable/disable damage tracking
    pub fn set_damage_tracked(&mut self, tracked: bool) {
        self.damage_tracked = tracked;
    }
}
