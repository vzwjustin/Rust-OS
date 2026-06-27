//! GNetworkMonitorNM matching `gio/gnetworkmonitornm.h`.
//! NetworkManager-based network monitor. In this no_std port we model
//! it with NM connection state.
//! Fully `no_std` compatible using `alloc`.

use spin::Mutex;

/// NetworkManager connectivity state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NMConnectivity {
    Unknown,
    None,
    Portal,
    Limited,
    Full,
}

/// A NetworkManager-based network monitor (`GNetworkMonitorNM`).
pub struct NetworkMonitorNM {
    connectivity: Mutex<NMConnectivity>,
    connected: Mutex<bool>,
}

impl NetworkMonitorNM {
    pub fn new() -> Self {
        Self {
            connectivity: Mutex::new(NMConnectivity::Unknown),
            connected: Mutex::new(false),
        }
    }

    pub fn get_connectivity(&self) -> NMConnectivity {
        *self.connectivity.lock()
    }
    pub fn set_connectivity(&self, c: NMConnectivity) {
        *self.connectivity.lock() = c;
        *self.connected.lock() = c == NMConnectivity::Full;
    }

    pub fn is_network_available(&self) -> bool {
        *self.connected.lock()
    }
    pub fn is_connected(&self) -> bool {
        *self.connected.lock()
    }
}

impl Default for NetworkMonitorNM {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let m = NetworkMonitorNM::new();
        assert_eq!(m.get_connectivity(), NMConnectivity::Unknown);
        assert!(!m.is_network_available());
    }

    #[test]
    fn test_full_connectivity() {
        let m = NetworkMonitorNM::new();
        m.set_connectivity(NMConnectivity::Full);
        assert!(m.is_network_available());
    }
}
