//! GIO proxy address matching `gio/gproxyaddress.h` /
//! `gio/gproxyaddress.c`.
//!
//! Upstream `GProxyAddress` is a `GObject` subclass that extends
//! `GInetSocketAddress` with proxy-related fields (protocol, destination
//! hostname/port, username, password, URI). We port it as a plain
//! `pub struct` that wraps `InetSocketAddress` and adds the proxy fields
//! rather than a registered GObject subclass, mirroring upstream semantics
//! with idiomatic Rust.
//!
//! Provides:
//! - `ProxyAddress` struct (InetSocketAddress + protocol + dest_hostname +
//!   dest_port + username + password + uri + dest_protocol).
//! - `new(inetaddr, port, protocol, dest_hostname, dest_port, username, password)`.
//! - `protocol()`, `destination_protocol()`, `destination_hostname()`,
//!   `destination_port()`, `username()`, `password()`, `uri()`.
//! - Delegated accessors: `address()`, `port()`, `family()`, `native_size()`,
//!   `to_native()`, `to_string()`.
//! - `equal()`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::ginetaddress::{InetAddress, SocketFamily};
use crate::ginetsocketaddress::InetSocketAddress;
use crate::gioerror::IOErrorEnum;
use alloc::string::String;
use alloc::string::ToString;

/// A proxy address (`GProxyAddress`).
///
/// An `InetSocketAddress` representing a connection via a proxy server.
/// Contains the proxy server's address/port plus the destination
/// hostname/port and authentication credentials.
///
/// Plain struct port of the upstream GObject subclass extending
/// `GInetSocketAddress`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProxyAddress {
    /// The underlying inet socket address (proxy server address + port).
    inet: InetSocketAddress,
    /// The proxy protocol (e.g. "socks", "http").
    protocol: String,
    /// The destination protocol (e.g. "http", "ftp"), if known.
    dest_protocol: Option<String>,
    /// The destination hostname the proxy should tunnel to.
    dest_hostname: String,
    /// The destination port to tunnel to.
    dest_port: u16,
    /// The username to authenticate to the proxy server.
    username: Option<String>,
    /// The password to authenticate to the proxy server.
    password: Option<String>,
    /// The URI string that the proxy was constructed from, if any.
    uri: Option<String>,
}

impl ProxyAddress {
    /// Creates a new `ProxyAddress` for the given proxy server address.
    ///
    /// Note: `uri` and `dest_protocol` are not set by this constructor
    /// (matching upstream `g_proxy_address_new`). Use `new_full` to set
    /// all fields.
    ///
    /// Mirrors `g_proxy_address_new`.
    pub fn new(
        inetaddr: InetAddress,
        port: u16,
        protocol: &str,
        dest_hostname: &str,
        dest_port: u16,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Self {
        ProxyAddress {
            inet: InetSocketAddress::new(inetaddr, port),
            protocol: protocol.to_string(),
            dest_protocol: None,
            dest_hostname: dest_hostname.to_string(),
            dest_port,
            username: username.map(|s| s.to_string()),
            password: password.map(|s| s.to_string()),
            uri: None,
        }
    }

    /// Creates a new `ProxyAddress` with all fields set, including
    /// `destination_protocol` and `uri`.
    pub fn new_full(
        inetaddr: InetAddress,
        port: u16,
        protocol: &str,
        dest_hostname: &str,
        dest_port: u16,
        username: Option<&str>,
        password: Option<&str>,
        dest_protocol: Option<&str>,
        uri: Option<&str>,
    ) -> Self {
        ProxyAddress {
            inet: InetSocketAddress::new(inetaddr, port),
            protocol: protocol.to_string(),
            dest_protocol: dest_protocol.map(|s| s.to_string()),
            dest_hostname: dest_hostname.to_string(),
            dest_port,
            username: username.map(|s| s.to_string()),
            password: password.map(|s| s.to_string()),
            uri: uri.map(|s| s.to_string()),
        }
    }

    /// Gets the proxy protocol (e.g. "socks", "http").
    ///
    /// Mirrors `g_proxy_address_get_protocol`.
    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    /// Gets the destination protocol (e.g. "http", "ftp"), if known.
    ///
    /// Mirrors `g_proxy_address_get_destination_protocol`.
    pub fn destination_protocol(&self) -> Option<&str> {
        self.dest_protocol.as_deref()
    }

    /// Gets the destination hostname.
    ///
    /// Mirrors `g_proxy_address_get_destination_hostname`.
    pub fn destination_hostname(&self) -> &str {
        &self.dest_hostname
    }

    /// Gets the destination port.
    ///
    /// Mirrors `g_proxy_address_get_destination_port`.
    pub fn destination_port(&self) -> u16 {
        self.dest_port
    }

    /// Gets the username for proxy authentication.
    ///
    /// Mirrors `g_proxy_address_get_username`.
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    /// Gets the password for proxy authentication.
    ///
    /// Mirrors `g_proxy_address_get_password`.
    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    /// Gets the URI string that the proxy was constructed from.
    ///
    /// Mirrors `g_proxy_address_get_uri`.
    pub fn uri(&self) -> Option<&str> {
        self.uri.as_deref()
    }

    // ── Delegated InetSocketAddress accessors ───────────────────────

    /// Gets the proxy server's `InetAddress`.
    pub fn address(&self) -> &InetAddress {
        self.inet.address()
    }

    /// Gets the proxy server's port.
    pub fn port(&self) -> u16 {
        self.inet.port()
    }

    /// Gets the socket family.
    pub fn family(&self) -> SocketFamily {
        self.inet.family()
    }

    /// Gets the native size of the underlying inet socket address.
    pub fn native_size(&self) -> usize {
        self.inet.native_size()
    }

    /// Serializes the underlying inet socket address to native bytes.
    pub fn to_native(&self, dest: &mut [u8]) -> Result<(), IOErrorEnum> {
        self.inet.to_native(dest)
    }

    /// Returns the connectable string representation of the underlying
    /// inet socket address.
    pub fn to_string(&self) -> String {
        self.inet.to_string()
    }

    /// Compares two `ProxyAddress` values for equality.
    pub fn equal(&self, other: &Self) -> bool {
        self.inet.equal(&other.inet)
            && self.protocol == other.protocol
            && self.dest_protocol == other.dest_protocol
            && self.dest_hostname == other.dest_hostname
            && self.dest_port == other.dest_port
            && self.username == other.username
            && self.password == other.password
            && self.uri == other.uri
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_addr() -> InetAddress {
        InetAddress::new_from_string("192.168.1.1").unwrap()
    }

    #[test]
    fn test_new() {
        let addr = make_addr();
        let pa = ProxyAddress::new(
            addr.clone(),
            1080,
            "socks",
            "example.com",
            443,
            Some("user"),
            Some("pass"),
        );
        assert_eq!(pa.protocol(), "socks");
        assert_eq!(pa.destination_hostname(), "example.com");
        assert_eq!(pa.destination_port(), 443);
        assert_eq!(pa.username(), Some("user"));
        assert_eq!(pa.password(), Some("pass"));
        assert_eq!(pa.uri(), None);
        assert_eq!(pa.destination_protocol(), None);
    }

    #[test]
    fn test_new_no_auth() {
        let addr = make_addr();
        let pa = ProxyAddress::new(addr.clone(), 8080, "http", "example.com", 80, None, None);
        assert_eq!(pa.username(), None);
        assert_eq!(pa.password(), None);
    }

    #[test]
    fn test_new_full() {
        let addr = make_addr();
        let pa = ProxyAddress::new_full(
            addr.clone(),
            1080,
            "socks",
            "example.com",
            443,
            Some("user"),
            Some("pass"),
            Some("https"),
            Some("socks://192.168.1.1:1080"),
        );
        assert_eq!(pa.destination_protocol(), Some("https"));
        assert_eq!(pa.uri(), Some("socks://192.168.1.1:1080"));
    }

    #[test]
    fn test_delegated_accessors() {
        let addr = make_addr();
        let pa = ProxyAddress::new(addr.clone(), 1080, "socks", "example.com", 443, None, None);
        assert_eq!(pa.address().to_string(), "192.168.1.1");
        assert_eq!(pa.port(), 1080);
        assert_eq!(pa.family(), SocketFamily::Ipv4);
        assert_eq!(pa.native_size(), 16);
    }

    #[test]
    fn test_to_string() {
        let addr = make_addr();
        let pa = ProxyAddress::new(addr.clone(), 1080, "socks", "example.com", 443, None, None);
        assert_eq!(pa.to_string(), "192.168.1.1:1080");
    }

    #[test]
    fn test_to_native() {
        let addr = make_addr();
        let pa = ProxyAddress::new(addr.clone(), 1080, "socks", "example.com", 443, None, None);
        let mut buf = [0u8; 16];
        assert!(pa.to_native(&mut buf).is_ok());
    }

    #[test]
    fn test_to_native_no_space() {
        let addr = make_addr();
        let pa = ProxyAddress::new(addr.clone(), 1080, "socks", "example.com", 443, None, None);
        let mut buf = [0u8; 4];
        assert_eq!(pa.to_native(&mut buf), Err(IOErrorEnum::NoSpace));
    }

    #[test]
    fn test_equal() {
        let addr = make_addr();
        let a = ProxyAddress::new(
            addr.clone(),
            1080,
            "socks",
            "example.com",
            443,
            Some("user"),
            Some("pass"),
        );
        let b = ProxyAddress::new(
            addr.clone(),
            1080,
            "socks",
            "example.com",
            443,
            Some("user"),
            Some("pass"),
        );
        assert!(a.equal(&b));
    }

    #[test]
    fn test_not_equal_different_protocol() {
        let addr = make_addr();
        let a = ProxyAddress::new(addr.clone(), 1080, "socks", "example.com", 443, None, None);
        let b = ProxyAddress::new(addr.clone(), 1080, "http", "example.com", 443, None, None);
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_not_equal_different_dest() {
        let addr = make_addr();
        let a = ProxyAddress::new(addr.clone(), 1080, "socks", "example.com", 443, None, None);
        let b = ProxyAddress::new(addr.clone(), 1080, "socks", "other.com", 443, None, None);
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_not_equal_different_port() {
        let addr = make_addr();
        let a = ProxyAddress::new(addr.clone(), 1080, "socks", "example.com", 443, None, None);
        let b = ProxyAddress::new(addr.clone(), 1080, "socks", "example.com", 80, None, None);
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_not_equal_different_username() {
        let addr = make_addr();
        let a = ProxyAddress::new(
            addr.clone(),
            1080,
            "socks",
            "example.com",
            443,
            Some("user"),
            None,
        );
        let b = ProxyAddress::new(addr.clone(), 1080, "socks", "example.com", 443, None, None);
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_clone() {
        let addr = make_addr();
        let a = ProxyAddress::new(
            addr.clone(),
            1080,
            "socks",
            "example.com",
            443,
            Some("user"),
            Some("pass"),
        );
        let b = a.clone();
        assert!(a.equal(&b));
    }

    #[test]
    fn test_ipv6() {
        let addr = InetAddress::new_from_string("2001:db8::1").unwrap();
        let pa = ProxyAddress::new(addr.clone(), 1080, "socks", "example.com", 443, None, None);
        assert_eq!(pa.family(), SocketFamily::Ipv6);
        assert_eq!(pa.native_size(), 28);
        assert_eq!(pa.to_string(), "[2001:db8::1]:1080");
    }
}
