//! Server-side compositor ported from `meta-compositor-server.c`.
//!
//! Manages server-side composition for Wayland protocol.

use crate::desktop::window_manager::WindowId;

/// Server-side compositor managing Wayland composition
#[derive(Debug)]
pub struct CompositorServer {
    pub id: u32,
    pub active: bool,
    pub client_count: usize,
}

impl CompositorServer {
    /// Create new server-side compositor
    pub fn new(id: u32) -> Self {
        CompositorServer {
            id,
            active: false,
            client_count: 0,
        }
    }

    /// Activate server compositor
    pub fn activate(&mut self) -> bool {
        self.active = true;
        true
    }

    /// Add Wayland client
    pub fn add_client(&mut self) -> bool {
        self.client_count += 1;
        true
    }

    /// Remove Wayland client
    pub fn remove_client(&mut self) -> bool {
        if self.client_count > 0 {
            self.client_count -= 1;
            true
        } else {
            false
        }
    }

    /// Translate monotonic time to X server time format
    pub fn monotonic_to_xserver_time(&self, monotonic_us: u64) -> u64 {
        // Convert from microseconds to milliseconds (X server time units)
        monotonic_us / 1000
    }

    /// Get active client count
    pub fn get_client_count(&self) -> usize {
        self.client_count
    }

    /// Check if compositor is active
    pub fn is_active(&self) -> bool {
        self.active
    }
}
