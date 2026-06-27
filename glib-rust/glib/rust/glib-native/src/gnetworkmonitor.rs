//! GNetworkMonitor matching `gio/gnetworkmonitor.h`.
//!
//! Monitors network connectivity. In this `no_std` port connectivity state
//! is a simple flag; no OS event loop is required.
//!
//! No_std compatible using `alloc`.

use spin::Mutex;

/// Network connectivity level (`GNetworkConnectivity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetworkConnectivity {
    /// No network access.
    Local = 1,
    /// Only limited connectivity (captive portal).
    Limited = 2,
    /// A portal is blocking full connectivity.
    Portal = 3,
    /// Full network connectivity.
    Full = 4,
}

/// Network availability monitor (`GNetworkMonitor`).
pub struct NetworkMonitor {
    connectivity: Mutex<NetworkConnectivity>,
    network_metered: Mutex<bool>,
}

impl NetworkMonitor {
    /// Creates a monitor that starts as fully connected.
    ///
    /// Mirrors `g_network_monitor_get_default` (singleton; here we allow
    /// multiple instances for testability).
    pub fn new() -> Self {
        Self {
            connectivity: Mutex::new(NetworkConnectivity::Full),
            network_metered: Mutex::new(false),
        }
    }

    /// Returns true if the network is available (connectivity >= Limited).
    ///
    /// Mirrors `g_network_monitor_get_network_available`.
    pub fn get_network_available(&self) -> bool {
        *self.connectivity.lock() >= NetworkConnectivity::Limited
    }

    /// Returns the current connectivity level.
    ///
    /// Mirrors `g_network_monitor_get_connectivity`.
    pub fn get_connectivity(&self) -> NetworkConnectivity {
        *self.connectivity.lock()
    }

    /// Sets the connectivity level (used by platform layer or tests).
    pub fn set_connectivity(&self, c: NetworkConnectivity) {
        *self.connectivity.lock() = c;
    }

    /// Returns true if the network connection is metered.
    ///
    /// Mirrors `g_network_monitor_get_network_metered`.
    pub fn get_network_metered(&self) -> bool {
        *self.network_metered.lock()
    }

    /// Sets the metered flag.
    pub fn set_network_metered(&self, metered: bool) {
        *self.network_metered.lock() = metered;
    }

    /// Returns true if the host is reachable (connectivity is Full).
    ///
    /// Mirrors `g_network_monitor_can_reach` (simplified: no DNS lookup).
    pub fn can_reach(&self, _host: &str) -> bool {
        *self.connectivity.lock() == NetworkConnectivity::Full
    }
}

impl Default for NetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_full_connectivity() {
        let m = NetworkMonitor::new();
        assert!(m.get_network_available());
        assert_eq!(m.get_connectivity(), NetworkConnectivity::Full);
        assert!(!m.get_network_metered());
    }

    #[test]
    fn test_set_connectivity_local() {
        let m = NetworkMonitor::new();
        m.set_connectivity(NetworkConnectivity::Local);
        assert!(!m.get_network_available());
        assert!(!m.can_reach("example.com"));
    }

    #[test]
    fn test_set_connectivity_limited() {
        let m = NetworkMonitor::new();
        m.set_connectivity(NetworkConnectivity::Limited);
        assert!(m.get_network_available());
        assert!(!m.can_reach("example.com")); // not Full
    }

    #[test]
    fn test_can_reach_full() {
        let m = NetworkMonitor::new();
        assert!(m.can_reach("example.com"));
    }

    #[test]
    fn test_metered() {
        let m = NetworkMonitor::new();
        m.set_network_metered(true);
        assert!(m.get_network_metered());
        m.set_network_metered(false);
        assert!(!m.get_network_metered());
    }

    #[test]
    fn test_connectivity_ordering() {
        assert!(NetworkConnectivity::Full > NetworkConnectivity::Portal);
        assert!(NetworkConnectivity::Limited > NetworkConnectivity::Local);
    }

    #[test]
    fn test_default() {
        let m = NetworkMonitor::default();
        assert_eq!(m.get_connectivity(), NetworkConnectivity::Full);
    }
}
