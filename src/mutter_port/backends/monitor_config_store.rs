//! Monitor Config Store — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-monitor-config-store.h

use alloc::string::String;

/// MetaMonitorConfigPolicy
#[derive(Debug, Clone)]
pub struct MetaMonitorConfigPolicy {
    pub enable_dbus: bool,
}

impl MetaMonitorConfigPolicy {
    /// Create a new monitor config policy.
    pub fn _new(&self) -> Self {
        Self {
            enable_dbus: self.enable_dbus,
        }
    }

    /// Look up a stored monitor configuration by key.
    /// Without a persistent store backend, returns None.
    pub fn _lookup(&self) -> Option<Self> {
        None
    }
}
