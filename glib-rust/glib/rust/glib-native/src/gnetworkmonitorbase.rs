//! GNetworkMonitorBase matching `gio/gnetworkmonitorbase.h`.
//! A base network monitor. In this no_std port we model it with
//! connectivity state and network availability.
//! Fully `no_std` compatible using `alloc`.

use spin::Mutex;

/// Network connectivity state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkConnectivity {
    Local,
    Limited,
    Full,
}

/// A base network monitor (`GNetworkMonitorBase`).
pub struct NetworkMonitorBase {
    available: Mutex<bool>,
    connectivity: Mutex<NetworkConnectivity>,
    metered: Mutex<bool>,
}

impl NetworkMonitorBase {
    pub fn new() -> Self {
        Self {
            available: Mutex::new(false),
            connectivity: Mutex::new(NetworkConnectivity::Local),
            metered: Mutex::new(false),
        }
    }

    pub fn get_network_available(&self) -> bool {
        *self.available.lock()
    }
    pub fn set_network_available(&self, available: bool) {
        *self.available.lock() = available;
    }

    pub fn get_connectivity(&self) -> NetworkConnectivity {
        *self.connectivity.lock()
    }
    pub fn set_connectivity(&self, c: NetworkConnectivity) {
        *self.connectivity.lock() = c;
    }

    pub fn get_network_metered(&self) -> bool {
        *self.metered.lock()
    }
    pub fn set_network_metered(&self, metered: bool) {
        *self.metered.lock() = metered;
    }

    pub fn can_reach(&self, _host: &str, _port: u16) -> bool {
        *self.available.lock() && *self.connectivity.lock() != NetworkConnectivity::Local
    }
}

impl Default for NetworkMonitorBase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let m = NetworkMonitorBase::new();
        assert!(!m.get_network_available());
        assert_eq!(m.get_connectivity(), NetworkConnectivity::Local);
    }

    #[test]
    fn test_available_reach() {
        let m = NetworkMonitorBase::new();
        m.set_network_available(true);
        m.set_connectivity(NetworkConnectivity::Full);
        assert!(m.can_reach("example.com", 80));
    }
}
