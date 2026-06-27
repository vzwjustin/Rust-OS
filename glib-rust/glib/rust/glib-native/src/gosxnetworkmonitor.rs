//! gosxnetworkmonitor matching `gio/gosxnetworkmonitor.c`.
//!
//! macOS network monitor using a `PF_ROUTE` socket and `sysctl` routing table
//! dumps to track connectivity. In this `no_std` port we model route socket
//! I/O and sysctl reads abstractly — no actual macOS syscalls.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use spin::Mutex;

/// Network connectivity level (`GNetworkConnectivity`), from `gnetworkmonitor`.
pub use crate::gnetworkmonitor::NetworkConnectivity;

/// Socket family for route entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketFamily {
    Ipv4,
    Ipv6,
    Unspecified,
}

/// A network route entry (mirrors `GInetAddressMask` data used by the C monitor).
#[derive(Debug, Clone)]
pub struct RouteEntry {
    pub family: SocketFamily,
    pub destination: Vec<u8>,
    pub prefix_length: u8,
}

/// Route change notification type (mirrors `RTM_*` message types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteChangeType {
    Add,
    Delete,
    InitialNotification,
}

/// macOS network monitor (`GOsxNetworkMonitor`).
pub struct OsxNetworkMonitor {
    initialized: Mutex<bool>,
    routes: Mutex<Vec<RouteEntry>>,
    connectivity: Mutex<NetworkConnectivity>,
    /// Simulated `PF_ROUTE` socket fd (`-1` when inactive).
    sockfd: Mutex<i32>,
    monitoring: Mutex<bool>,
    init_error: Mutex<Option<String>>,
}

impl OsxNetworkMonitor {
    /// Creates an uninitialized monitor.
    ///
    /// Mirrors `g_osx_network_monitor_init`.
    pub fn new() -> Self {
        Self {
            initialized: Mutex::new(false),
            routes: Mutex::new(Vec::new()),
            connectivity: Mutex::new(NetworkConnectivity::Local),
            sockfd: Mutex::new(-1),
            monitoring: Mutex::new(false),
            init_error: Mutex::new(None),
        }
    }

    /// Initializes the monitor: reads the routing table and starts route monitoring.
    ///
    /// Mirrors `g_osx_network_monitor_initable_init`.
    pub fn init(&self) -> bool {
        if !self.start_monitoring() {
            return false;
        }
        self.update_connectivity();
        *self.initialized.lock() = true;
        true
    }

    pub fn is_initialized(&self) -> bool {
        *self.initialized.lock()
    }

    pub fn init_error(&self) -> Option<String> {
        self.init_error.lock().clone()
    }

    /// Simulated route socket fd (`-1` when not monitoring).
    pub fn route_socket_fd(&self) -> i32 {
        *self.sockfd.lock()
    }

    pub fn is_monitoring(&self) -> bool {
        *self.monitoring.lock()
    }

    /// Replaces the route list from a simulated `sysctl` routing table dump.
    ///
    /// Mirrors `osx_network_manager_process_table`.
    pub fn process_table(&self, routes: Vec<RouteEntry>) {
        let filtered: Vec<RouteEntry> = routes
            .into_iter()
            .filter(|r| r.family != SocketFamily::Ipv6)
            .collect();
        *self.routes.lock() = filtered;
        self.update_connectivity();
    }

    /// Adds a route (IPv6 routes are ignored, matching C `get_network_mask`).
    ///
    /// Mirrors `g_network_monitor_base_add_network` on `RTM_ADD`.
    pub fn add_route(&self, route: RouteEntry) {
        if route.family == SocketFamily::Ipv6 {
            return;
        }
        self.routes.lock().push(route);
        self.update_connectivity();
    }

    /// Removes a matching route.
    ///
    /// Mirrors `g_network_monitor_base_remove_network` on `RTM_DELETE`.
    pub fn remove_route(&self, family: SocketFamily, destination: &[u8], prefix_length: u8) {
        self.routes.lock().retain(|r| {
            !(r.family == family
                && r.destination == destination
                && r.prefix_length == prefix_length)
        });
        self.update_connectivity();
    }

    /// Handles a simulated route socket notification.
    ///
    /// Mirrors `osx_network_monitor_callback`.
    pub fn on_route_change(&self, change_type: RouteChangeType, route: RouteEntry) {
        match change_type {
            RouteChangeType::Add => self.add_route(route),
            RouteChangeType::Delete => {
                self.remove_route(route.family, &route.destination, route.prefix_length);
            }
            RouteChangeType::InitialNotification => {}
        }
    }

    /// Returns the current route list.
    pub fn routes(&self) -> Vec<RouteEntry> {
        self.routes.lock().clone()
    }

    /// Returns whether the network is available (at least one IPv4 route).
    ///
    /// Mirrors `g_network_monitor_get_network_available` via base networks list.
    pub fn is_network_available(&self) -> bool {
        !self.routes.lock().is_empty() && *self.connectivity.lock() >= NetworkConnectivity::Limited
    }

    /// Returns the current connectivity level.
    pub fn get_connectivity(&self) -> NetworkConnectivity {
        *self.connectivity.lock()
    }

    /// Sets connectivity explicitly (for tests or platform overrides).
    pub fn set_connectivity(&self, connectivity: NetworkConnectivity) {
        *self.connectivity.lock() = connectivity;
    }

    fn start_monitoring(&self) -> bool {
        *self.sockfd.lock() = 1;
        *self.monitoring.lock() = true;
        true
    }

    fn update_connectivity(&self) {
        let connectivity = if self.routes.lock().is_empty() {
            NetworkConnectivity::Local
        } else {
            NetworkConnectivity::Full
        };
        *self.connectivity.lock() = connectivity;
    }
}

impl Default for OsxNetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the position of the last set bit in an IP address.
///
/// Mirrors C `get_last_bit_position` (used to derive prefix length from netmask).
pub fn get_last_bit_position(ip: &[u8], len_in_bits: usize) -> usize {
    let bytes = (len_in_bits / 8).min(ip.len());
    let mut ip_in_binary: u64 = 0;
    for i in 0..bytes {
        ip_in_binary = (ip_in_binary << 8) | u64::from(ip[i]);
    }
    if ip_in_binary == 0 {
        return 0;
    }
    let lsf = ip_in_binary.trailing_zeros() as usize;
    len_in_bits - lsf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ipv4_route(dest: &[u8], prefix: u8) -> RouteEntry {
        RouteEntry {
            family: SocketFamily::Ipv4,
            destination: dest.to_vec(),
            prefix_length: prefix,
        }
    }

    #[test]
    fn test_init() {
        let monitor = OsxNetworkMonitor::new();
        assert!(!monitor.is_initialized());
        assert_eq!(monitor.route_socket_fd(), -1);
        assert!(monitor.init());
        assert!(monitor.is_initialized());
        assert!(monitor.is_monitoring());
        assert_eq!(monitor.route_socket_fd(), 1);
    }

    #[test]
    fn test_add_remove_route() {
        let monitor = OsxNetworkMonitor::new();
        let route = ipv4_route(&[192, 168, 1, 0], 24);
        monitor.add_route(route.clone());
        assert_eq!(monitor.routes().len(), 1);
        assert!(monitor.is_network_available());
        assert_eq!(monitor.get_connectivity(), NetworkConnectivity::Full);

        monitor.remove_route(SocketFamily::Ipv4, &[192, 168, 1, 0], 24);
        assert_eq!(monitor.routes().len(), 0);
        assert!(!monitor.is_network_available());
        assert_eq!(monitor.get_connectivity(), NetworkConnectivity::Local);
    }

    #[test]
    fn test_on_route_change_add() {
        let monitor = OsxNetworkMonitor::new();
        monitor.on_route_change(RouteChangeType::Add, ipv4_route(&[10, 0, 0, 0], 8));
        assert_eq!(monitor.routes().len(), 1);
        assert!(monitor.is_network_available());
    }

    #[test]
    fn test_on_route_change_delete() {
        let monitor = OsxNetworkMonitor::new();
        let route = ipv4_route(&[172, 16, 0, 0], 12);
        monitor.on_route_change(RouteChangeType::Add, route.clone());
        monitor.on_route_change(RouteChangeType::Delete, route);
        assert_eq!(monitor.routes().len(), 0);
        assert!(!monitor.is_network_available());
    }

    #[test]
    fn test_on_route_change_initial_notification() {
        let monitor = OsxNetworkMonitor::new();
        monitor.on_route_change(
            RouteChangeType::InitialNotification,
            ipv4_route(&[0, 0, 0, 0], 0),
        );
        assert_eq!(monitor.routes().len(), 0);
    }

    #[test]
    fn test_process_table_skips_ipv6() {
        let monitor = OsxNetworkMonitor::new();
        monitor.process_table(vec![
            ipv4_route(&[192, 168, 0, 0], 16),
            RouteEntry {
                family: SocketFamily::Ipv6,
                destination: vec![0xfe, 0x80, 0, 0, 0, 0, 0, 0],
                prefix_length: 64,
            },
        ]);
        assert_eq!(monitor.routes().len(), 1);
        assert!(monitor.is_network_available());
    }

    #[test]
    fn test_get_last_bit_position() {
        assert_eq!(get_last_bit_position(&[0, 0, 0, 0], 32), 0);
        assert_eq!(get_last_bit_position(&[255, 255, 255, 255], 32), 32);
        assert_eq!(get_last_bit_position(&[0, 0, 0, 1], 32), 32);
        assert_eq!(get_last_bit_position(&[32, 0, 0, 0], 32), 3);
    }
}
