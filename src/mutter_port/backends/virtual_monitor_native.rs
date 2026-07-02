//! Virtual Monitor Native — ported from GNOME Mutter
//!
//! Represents a virtual monitor managed natively by the display backend.
//! Provides ID tracking and construction for virtual monitors.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-virtual-monitor-native.h

use core::mem;

/// Opaque virtual monitor native type.
/// Wraps a backend-specific virtual monitor implementation.
pub struct MetaVirtualMonitorNative {
    id: u64,
}

impl MetaVirtualMonitorNative {
    /// Create a new virtual monitor native instance.
    pub fn new(id: u64) -> Self {
        MetaVirtualMonitorNative { id }
    }

    /// Get the unique ID of this virtual monitor.
    pub fn get_id(&self) -> u64 {
        self.id
    }
}

impl Default for MetaVirtualMonitorNative {
    fn default() -> Self {
        Self::new(0)
    }
}
