//! GNetworkMonitorNetlink matching `gio/gnetworkmonitornetlink.h`.
//! Netlink-based network monitor. In this no_std port we model it with
//! a list of network routes and link state.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A netlink-based network monitor (`GNetworkMonitorNetlink`).
pub struct NetworkMonitorNetlink {
    links: Mutex<Vec<String>>,
    routes: Mutex<Vec<String>>,
    available: Mutex<bool>,
}

impl NetworkMonitorNetlink {
    pub fn new() -> Self {
        Self {
            links: Mutex::new(Vec::new()),
            routes: Mutex::new(Vec::new()),
            available: Mutex::new(false),
        }
    }

    pub fn add_link(&self, name: &str) {
        self.links.lock().push(name.to_string());
    }
    pub fn add_route(&self, route: &str) {
        self.routes.lock().push(route.to_string());
    }
    pub fn get_links(&self) -> Vec<String> {
        self.links.lock().clone()
    }
    pub fn get_routes(&self) -> Vec<String> {
        self.routes.lock().clone()
    }

    pub fn is_network_available(&self) -> bool {
        *self.available.lock() && !self.links.lock().is_empty()
    }
    pub fn set_available(&self, available: bool) {
        *self.available.lock() = available;
    }

    pub fn link_count(&self) -> usize {
        self.links.lock().len()
    }
    pub fn route_count(&self) -> usize {
        self.routes.lock().len()
    }
}

impl Default for NetworkMonitorNetlink {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_links_routes() {
        let m = NetworkMonitorNetlink::new();
        m.add_link("eth0");
        m.add_route("default via 192.168.1.1");
        m.set_available(true);
        assert!(m.is_network_available());
        assert_eq!(m.link_count(), 1);
        assert_eq!(m.route_count(), 1);
    }
}
