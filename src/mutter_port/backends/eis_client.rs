//! Eis Client — Per-client EIS connection handler from GNOME Mutter
//!
//! Wraps a libeis client connection and processes EIS events.
//! Event parsing and device routing are left as TODO.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-eis-client.h

/// MetaEisClient — Per-client EIS connection.
/// Processes inbound events from a libeis client socket.
pub struct MetaEisClient {
    // TODO: port fields from meta-eis-client.c (libeis client handle, etc.)
}

impl MetaEisClient {
    pub fn new() -> Self {
        MetaEisClient {}
    }
}

impl Default for MetaEisClient {
    fn default() -> Self {
        Self::new()
    }
}
