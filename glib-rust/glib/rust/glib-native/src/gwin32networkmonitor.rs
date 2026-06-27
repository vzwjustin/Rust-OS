//! gwin32networkmonitor matching `gio/gwin32networkmonitor.c`.
//!
//! Windows network monitor using the IP Helper API (`GetIpForwardTable2`)
//! to track routing table changes and report network connectivity.
//!
//! In this no_std port, we model the network list and route change
//! notifications abstractly.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use spin::Mutex;

/// Socket family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketFamily {
    Ipv4,
    Ipv6,
    Invalid,
}

/// A network route entry.
#[derive(Debug, Clone)]
pub struct RouteEntry {
    pub family: SocketFamily,
    pub destination: Vec<u8>,
    pub prefix_length: u8,
}

/// Route change notification type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteChangeType {
    Add,
    Delete,
    InitialNotification,
}

/// Windows network monitor (`GWin32NetworkMonitor`).
pub struct Win32NetworkMonitor {
    initialized: Mutex<bool>,
    routes: Mutex<Vec<RouteEntry>>,
    init_error: Mutex<Option<String>>,
}

impl Win32NetworkMonitor {
    pub fn new() -> Self {
        Self {
            initialized: Mutex::new(false),
            routes: Mutex::new(Vec::new()),
            init_error: Mutex::new(None),
        }
    }

    /// Initializes the network monitor by reading the IP routing table.
    ///
    /// Mirrors `g_win32_network_monitor_initable_init`.
    pub fn init(&self) -> bool {
        *self.initialized.lock() = true;
        true
    }

    pub fn is_initialized(&self) -> bool {
        *self.initialized.lock()
    }

    pub fn init_error(&self) -> Option<String> {
        self.init_error.lock().clone()
    }

    /// Processes the current routing table.
    ///
    /// Mirrors `win_network_monitor_process_table`.
    pub fn process_table(&self, routes: Vec<RouteEntry>) {
        *self.routes.lock() = routes;
    }

    /// Adds a network route.
    pub fn add_network(&self, route: RouteEntry) {
        self.routes.lock().push(route);
    }

    /// Removes a network route.
    pub fn remove_network(&self, family: SocketFamily, destination: &[u8], prefix_length: u8) {
        self.routes.lock().retain(|r| {
            !(r.family == family
                && r.destination == destination
                && r.prefix_length == prefix_length)
        });
    }

    /// Handles a route change notification.
    pub fn route_changed(&self, change_type: RouteChangeType, route: RouteEntry) {
        match change_type {
            RouteChangeType::Add => self.add_network(route),
            RouteChangeType::Delete => {
                self.remove_network(route.family, &route.destination, route.prefix_length);
            }
            RouteChangeType::InitialNotification => {}
        }
    }

    /// Returns the current list of routes.
    pub fn routes(&self) -> Vec<RouteEntry> {
        self.routes.lock().clone()
    }

    /// Returns whether the network is available (has at least one route).
    pub fn network_available(&self) -> bool {
        !self.routes.lock().is_empty()
    }
}

impl Default for Win32NetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let monitor = Win32NetworkMonitor::new();
        assert!(!monitor.is_initialized());
        assert!(monitor.init());
        assert!(monitor.is_initialized());
    }

    #[test]
    fn test_add_remove_route() {
        let monitor = Win32NetworkMonitor::new();
        let route = RouteEntry {
            family: SocketFamily::Ipv4,
            destination: vec![192, 168, 1, 0],
            prefix_length: 24,
        };
        monitor.add_network(route.clone());
        assert_eq!(monitor.routes().len(), 1);
        assert!(monitor.network_available());

        monitor.remove_network(SocketFamily::Ipv4, &[192, 168, 1, 0], 24);
        assert_eq!(monitor.routes().len(), 0);
        assert!(!monitor.network_available());
    }

    #[test]
    fn test_route_changed_add() {
        let monitor = Win32NetworkMonitor::new();
        let route = RouteEntry {
            family: SocketFamily::Ipv6,
            destination: vec![0xfe, 0x80, 0, 0, 0, 0, 0, 0],
            prefix_length: 64,
        };
        monitor.route_changed(RouteChangeType::Add, route);
        assert_eq!(monitor.routes().len(), 1);
    }

    #[test]
    fn test_route_changed_delete() {
        let monitor = Win32NetworkMonitor::new();
        let route = RouteEntry {
            family: SocketFamily::Ipv4,
            destination: vec![10, 0, 0, 0],
            prefix_length: 8,
        };
        monitor.route_changed(RouteChangeType::Add, route.clone());
        monitor.route_changed(RouteChangeType::Delete, route);
        assert_eq!(monitor.routes().len(), 0);
    }

    #[test]
    fn test_route_changed_initial() {
        let monitor = Win32NetworkMonitor::new();
        monitor.route_changed(
            RouteChangeType::InitialNotification,
            RouteEntry {
                family: SocketFamily::Ipv4,
                destination: vec![0, 0, 0, 0],
                prefix_length: 0,
            },
        );
        assert_eq!(monitor.routes().len(), 0);
    }

    #[test]
    fn test_process_table() {
        let monitor = Win32NetworkMonitor::new();
        let routes = vec![
            RouteEntry {
                family: SocketFamily::Ipv4,
                destination: vec![192, 168, 0, 0],
                prefix_length: 16,
            },
            RouteEntry {
                family: SocketFamily::Ipv4,
                destination: vec![10, 0, 0, 0],
                prefix_length: 8,
            },
        ];
        monitor.process_table(routes);
        assert_eq!(monitor.routes().len(), 2);
    }
}
