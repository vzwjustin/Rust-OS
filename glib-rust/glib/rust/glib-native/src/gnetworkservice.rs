//! GIO network service matching `gio/gnetworkservice.h` /
//! `gio/gnetworkservice.c`.
//!
//! Upstream `GNetworkService` is a `GObject` subclass that implements
//! `GSocketConnectable`. It represents a SRV record to be resolved.
//! We port it as a plain `pub struct` with the same API, since the
//! GObject subclassing / interface system is deferred (Phase 9).
//!
//! Provides:
//! - `NetworkService` struct (service / protocol / domain / scheme).
//! - `new(service, protocol, domain)`.
//! - `service()`, `protocol()`, `domain()`, `scheme()`, `set_scheme()`.
//! - `to_string()` — connectable string `"(service, protocol, domain, scheme)"`.
//! - `equal()`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::String;
use alloc::string::ToString;

/// A network service representing a SRV record (`GNetworkService`).
///
/// Like `NetworkAddress` but for DNS SRV records — resolves a
/// `(service, protocol, domain)` triple to a set of host/port targets
/// with priority/weight ordering (RFC 2782).
///
/// Plain struct port of the upstream GObject+GSocketConnectable subclass.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NetworkService {
    service: String,
    protocol: String,
    domain: String,
    scheme: Option<String>,
}

impl NetworkService {
    /// Creates a new `NetworkService` representing the given service,
    /// protocol, and domain.
    ///
    /// Mirrors `g_network_service_new`.
    pub fn new(service: &str, protocol: &str, domain: &str) -> Self {
        NetworkService {
            service: service.to_string(),
            protocol: protocol.to_string(),
            domain: domain.to_string(),
            scheme: None,
        }
    }

    /// Gets the service name (e.g. `"ldap"`).
    ///
    /// Mirrors `g_network_service_get_service`.
    pub fn service(&self) -> &str {
        &self.service
    }

    /// Gets the protocol name (e.g. `"tcp"`).
    ///
    /// Mirrors `g_network_service_get_protocol`.
    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    /// Gets the domain name.
    ///
    /// Mirrors `g_network_service_get_domain`.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Gets the URI scheme used to resolve proxies. By default, the
    /// service name is used as the scheme.
    ///
    /// Mirrors `g_network_service_get_scheme`.
    pub fn scheme(&self) -> &str {
        match &self.scheme {
            Some(s) => s.as_str(),
            None => &self.service,
        }
    }

    /// Sets the URI scheme used to resolve proxies.
    ///
    /// Mirrors `g_network_service_set_scheme`.
    pub fn set_scheme(&mut self, scheme: &str) {
        self.scheme = Some(scheme.to_string());
    }

    /// Returns the connectable string representation.
    ///
    /// Format: `"(service, protocol, domain, scheme)"`.
    ///
    /// Mirrors `g_network_service_connectable_to_string`.
    pub fn to_string(&self) -> String {
        format!(
            "({}, {}, {}, {})",
            self.service,
            self.protocol,
            self.domain,
            self.scheme()
        )
    }

    /// Compares two `NetworkService` values for equality.
    pub fn equal(&self, other: &Self) -> bool {
        self.service == other.service
            && self.protocol == other.protocol
            && self.domain == other.domain
            && self.scheme == other.scheme
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let ns = NetworkService::new("ldap", "tcp", "example.com");
        assert_eq!(ns.service(), "ldap");
        assert_eq!(ns.protocol(), "tcp");
        assert_eq!(ns.domain(), "example.com");
    }

    #[test]
    fn test_scheme_default() {
        let ns = NetworkService::new("ldap", "tcp", "example.com");
        assert_eq!(ns.scheme(), "ldap");
    }

    #[test]
    fn test_set_scheme() {
        let mut ns = NetworkService::new("ldap", "tcp", "example.com");
        ns.set_scheme("ldaps");
        assert_eq!(ns.scheme(), "ldaps");
    }

    #[test]
    fn test_set_scheme_override() {
        let mut ns = NetworkService::new("http", "tcp", "example.com");
        ns.set_scheme("https");
        assert_eq!(ns.scheme(), "https");
        ns.set_scheme("ftp");
        assert_eq!(ns.scheme(), "ftp");
    }

    #[test]
    fn test_to_string_default_scheme() {
        let ns = NetworkService::new("ldap", "tcp", "example.com");
        assert_eq!(ns.to_string(), "(ldap, tcp, example.com, ldap)");
    }

    #[test]
    fn test_to_string_custom_scheme() {
        let mut ns = NetworkService::new("ldap", "tcp", "example.com");
        ns.set_scheme("ldaps");
        assert_eq!(ns.to_string(), "(ldap, tcp, example.com, ldaps)");
    }

    #[test]
    fn test_equal() {
        let a = NetworkService::new("ldap", "tcp", "example.com");
        let b = NetworkService::new("ldap", "tcp", "example.com");
        let c = NetworkService::new("http", "tcp", "example.com");
        assert!(a.equal(&b));
        assert!(!a.equal(&c));
    }

    #[test]
    fn test_equal_different_scheme() {
        let a = NetworkService::new("ldap", "tcp", "example.com");
        let mut b = NetworkService::new("ldap", "tcp", "example.com");
        b.set_scheme("ldaps");
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_equal_different_protocol() {
        let a = NetworkService::new("ldap", "tcp", "example.com");
        let b = NetworkService::new("ldap", "udp", "example.com");
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_equal_different_domain() {
        let a = NetworkService::new("ldap", "tcp", "example.com");
        let b = NetworkService::new("ldap", "tcp", "other.com");
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_clone() {
        let a = NetworkService::new("ldap", "tcp", "example.com");
        let b = a.clone();
        assert!(a.equal(&b));
    }

    #[test]
    fn test_clone_with_scheme() {
        let mut a = NetworkService::new("ldap", "tcp", "example.com");
        a.set_scheme("ldaps");
        let b = a.clone();
        assert!(a.equal(&b));
        assert_eq!(b.scheme(), "ldaps");
    }
}
