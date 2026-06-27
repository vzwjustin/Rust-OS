//! GNetworkMonitorPortal matching `gio/gnetworkmonitorportal.h`.
//! Portal-based network monitor. In this no_std port we model it with
//! portal availability and network state.
//! Fully `no_std` compatible using `alloc`.

use spin::Mutex;

/// A portal-based network monitor (`GNetworkMonitorPortal`).
pub struct NetworkMonitorPortal {
    available: Mutex<bool>,
    network_available: Mutex<bool>,
    metered: Mutex<bool>,
}

impl NetworkMonitorPortal {
    pub fn new() -> Self {
        Self {
            available: Mutex::new(false),
            network_available: Mutex::new(false),
            metered: Mutex::new(false),
        }
    }

    pub fn is_available(&self) -> bool {
        *self.available.lock()
    }
    pub fn set_available(&self, available: bool) {
        *self.available.lock() = available;
    }

    pub fn get_network_available(&self) -> bool {
        *self.network_available.lock()
    }
    pub fn set_network_available(&self, available: bool) {
        *self.network_available.lock() = available;
    }

    pub fn get_network_metered(&self) -> bool {
        *self.metered.lock()
    }
    pub fn set_network_metered(&self, metered: bool) {
        *self.metered.lock() = metered;
    }
}

impl Default for NetworkMonitorPortal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portal() {
        let m = NetworkMonitorPortal::new();
        m.set_available(true);
        m.set_network_available(true);
        assert!(m.is_available());
        assert!(m.get_network_available());
    }
}
