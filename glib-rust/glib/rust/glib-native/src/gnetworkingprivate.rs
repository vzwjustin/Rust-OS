//! `gnetworkingprivate` matching `gio/gnetworkingprivate.h`.
//!
//! Private networking API: resolver serial, socket creation,
//! and service-by-name lookup.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Address family (mirrors `AF_*` constants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressFamily {
    Unspec = 0,
    Inet = 2,
    Inet6 = 10,
    Unix = 1,
}

/// Socket type (mirrors `SOCK_*` constants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketKind {
    Stream = 1,
    Datagram = 2,
    Raw = 3,
}

/// Returns the resolver serial number (mirrors `g_resolver_get_serial`).
///
/// Increments on each call to simulate DNS cache invalidation.
pub fn resolver_get_serial() -> u64 {
    let mut serial = RESOLVER_SERIAL.lock();
    *serial += 1;
    *serial
}

/// Creates a socket (mirrors `g_socket`).
///
/// In our no_std port, we return a synthetic file descriptor.
pub fn g_socket(
    domain: AddressFamily,
    socket_type: SocketKind,
    protocol: i32,
) -> Result<i32, String> {
    if domain == AddressFamily::Unspec {
        return Err("address family must be specified".to_string());
    }
    let mut next = NEXT_FD.lock();
    let fd = *next;
    *next += 1;
    Ok(fd)
}

/// Looks up a service by name and returns the port in host byte order
/// (mirrors `g_getservbyname_ntohs`).
pub fn getservbyname_ntohs(name: &str, proto: &str) -> Option<u16> {
    let services = SERVICES.lock();
    services
        .iter()
        .find(|(n, p, _)| n == name && p == proto)
        .map(|(_, _, port)| *port)
}

/// Registers a service mapping (for testing).
pub fn register_service(name: &str, proto: &str, port: u16) {
    SERVICES
        .lock()
        .push((name.to_string(), proto.to_string(), port));
}

/// Clears all service mappings (for testing).
pub fn clear_services() {
    SERVICES.lock().clear();
}

static RESOLVER_SERIAL: Mutex<u64> = Mutex::new(0);
static NEXT_FD: Mutex<i32> = Mutex::new(3);
static SERVICES: Mutex<Vec<(String, String, u16)>> = Mutex::new(Vec::new());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_serial_increments() {
        let s1 = resolver_get_serial();
        let s2 = resolver_get_serial();
        assert!(s2 > s1);
    }

    #[test]
    fn test_socket_creation() {
        let fd = g_socket(AddressFamily::Inet, SocketKind::Stream, 0);
        assert!(fd.is_ok());
        assert!(fd.unwrap() >= 3);
    }

    #[test]
    fn test_socket_unspec_fails() {
        assert!(g_socket(AddressFamily::Unspec, SocketKind::Stream, 0).is_err());
    }

    #[test]
    fn test_getservbyname() {
        clear_services();
        register_service("http", "tcp", 80);
        register_service("https", "tcp", 443);
        assert_eq!(getservbyname_ntohs("http", "tcp"), Some(80));
        assert_eq!(getservbyname_ntohs("https", "tcp"), Some(443));
        assert_eq!(getservbyname_ntohs("ftp", "tcp"), None);
    }
}
