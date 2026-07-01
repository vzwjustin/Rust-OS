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
    /// TODO: port logic from meta_monitor_config_store_new
    pub fn _new(&self) {
        todo!()
    }

    /// TODO: port logic from meta_monitor_config_store_lookup
    pub fn _lookup(&self) {
        todo!()
    }

}
