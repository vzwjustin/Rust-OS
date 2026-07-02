//! Device Pool Private ported from GNOME Mutter's src/backends/
//!
//! Device lifecycle management for file descriptors and resources.
//! Minimal stub for upstream header not found.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-device-pool-private.h

// Upstream header not found; minimal stub.

/// Opaque device pool type.
pub struct MetaDevicePool;

impl MetaDevicePool {
    /// Create a new device pool.
    pub fn new() -> Self {
        MetaDevicePool
    }
}

impl Default for MetaDevicePool {
    fn default() -> Self {
        Self::new()
    }
}
