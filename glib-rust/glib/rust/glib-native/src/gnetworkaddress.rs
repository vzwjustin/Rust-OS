//! GIO network address matching `gio/gnetworkaddress.h` /
//! `gio/gnetworkaddress.c`.
//!
//! Upstream `GNetworkAddress` is a `GObject` subclass implementing
//! `GSocketConnectable`. We port it as a plain `pub struct` with the
//! same fields and API (minus the `GSocketConnectable` interface and
//! DNS resolution, which need the deferred GObject interface system
//! and a real resolver). The parse functions (`parse`,
//! `parse_uri`) are pure parsing logic and are fully implemented.
//!
//! Provides:
//! - `NetworkAddressError` enum matching the upstream
//!   `G_IO_ERROR_INVALID_ARGUMENT` cases.
//! - `NetworkAddress` struct (hostname, port, optional scheme) with
//!   `new`, `new_loopback`, `parse` (host:port + bracketed IPv6),
//!   `parse_uri` (scheme://host:port via `Uri::parse`), `hostname`,
//!   `port`, `scheme`, `equal`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use crate::uri::{Uri, UriFlags};
use alloc::string::{String, ToString};

// ─────────────────────── NetworkAddressError ──────────────────────────────

/// Errors returned by `NetworkAddress::parse` / `parse_uri`.
/// Mirrors the upstream `G_IO_ERROR_INVALID_ARGUMENT` cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NetworkAddressError {
    /// Hostname contains '[' but not ']'.
    UnclosedBracket,
    /// ']' must come at the end or be followed by ':' and a port.
    BadBracket,
    /// ':' given but not followed by a port.
    EmptyPort,
    /// Invalid numeric port (non-numeric, out of range).
    InvalidPort,
    /// Unknown service name (we don't have getservbyname in no_std).
    UnknownService,
    /// Invalid URI.
    InvalidUri,
}

// ──────────────────────── NetworkAddress ─────────────────────────────────

/// A network address (`GNetworkAddress`).
///
/// Plain struct port of the upstream GObject subclass. Holds a
/// hostname, port, and optional scheme (set by `parse_uri`). Upstream
/// also caches resolved `GSocketAddress`s; we skip that (needs DNS
/// resolution + the `GSocketConnectable` interface).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NetworkAddress {
    hostname: String,
    port: u16,
    scheme: Option<String>,
}

impl NetworkAddress {
    /// Create a new network address (`g_network_address_new`).
    pub fn new(hostname: &str, port: u16) -> Self {
        Self {
            hostname: hostname.to_owned(),
            port,
            scheme: None,
        }
    }

    /// Create a loopback network address (`g_network_address_new_loopback`).
    ///
    /// Hostname is `"localhost"`.
    pub fn new_loopback(port: u16) -> Self {
        Self::new("localhost", port)
    }

    /// Parse a `"host:port"` or `"[ipv6]:port"` string
    /// (`g_network_address_parse`).
    ///
    /// Accepts:
    /// - `"host"` — uses `default_port`.
    /// - `"host:port"` — numeric port.
    /// - `"[ipv6]:port"` — bracketed IPv6 with port.
    /// - `"[ipv6]"` — bracketed IPv6, uses `default_port`.
    /// - `"ipv6"` (with multiple `:`) — unescaped IPv6, uses `default_port`.
    ///
    /// Service names (e.g. `"http"`) are not supported in `no_std`
    /// (no `getservbyname`); returns `UnknownService`.
    pub fn parse(host_and_port: &str, default_port: u16) -> Result<Self, NetworkAddressError> {
        let hostname: String;
        let port_str: Option<&str>;

        if host_and_port.starts_with('[') {
            // Bracketed IPv6: [addr] or [addr]:port
            let end = host_and_port.find(']').ok_or(NetworkAddressError::UnclosedBracket)?;
            let after_bracket = &host_and_port[end + 1..];
            match after_bracket {
                "" => port_str = None,
                s if s.starts_with(':') => port_str = Some(&s[1..]),
                _ => return Err(NetworkAddressError::BadBracket),
            }
            hostname = host_and_port[1..end].to_string();
        } else if let Some(colon_idx) = host_and_port.find(':') {
            // Has at least one ':'. Check if there's a second ':' (IPv6).
            let after_colon = &host_and_port[colon_idx + 1..];
            if after_colon.contains(':') {
                // Multiple ':' → unescaped IPv6, no port.
                hostname = host_and_port.to_string();
                port_str = None;
            } else {
                // host:port
                hostname = host_and_port[..colon_idx].to_string();
                port_str = Some(after_colon);
            }
        } else {
            // Plain hostname, no port.
            hostname = host_and_port.to_string();
            port_str = None;
        }

        let port = if let Some(p) = port_str {
            if p.is_empty() {
                return Err(NetworkAddressError::EmptyPort);
            }
            // Must be numeric (we don't support service names in no_std).
            if !p.chars().all(|c| c.is_ascii_digit()) {
                // Could be a service name like "http" — but we can't
                // resolve those without getservbyname.
                if p.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                    // Starts with a digit but has non-digit chars —
                    // invalid numeric port.
                    return Err(NetworkAddressError::InvalidPort);
                }
                return Err(NetworkAddressError::UnknownService);
            }
            let value: u32 = p.parse().map_err(|_| NetworkAddressError::InvalidPort)?;
            if value > u16::MAX as u32 {
                return Err(NetworkAddressError::InvalidPort);
            }
            value as u16
        } else {
            default_port
        };

        Ok(Self { hostname, port, scheme: None })
    }

    /// Parse a URI of the form `"scheme://host:port"`
    /// (`g_network_address_parse_uri`).
    ///
    /// Uses the ported `Uri::parse` to extract scheme, host, and port.
    /// If the URI has no port, uses `default_port`.
    pub fn parse_uri(uri: &str, default_port: u16) -> Result<Self, NetworkAddressError> {
        let parsed = Uri::parse(uri, UriFlags::NONE).map_err(|_| NetworkAddressError::InvalidUri)?;
        let scheme = parsed.scheme().to_string();
        let hostname = parsed.host().to_string();
        let port = parsed.port().unwrap_or(default_port);
        Ok(Self {
            hostname,
            port,
            scheme: Some(scheme),
        })
    }

    /// Hostname (`g_network_address_get_hostname`).
    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    /// Port (`g_network_address_get_port`).
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Scheme, if built from a URI (`g_network_address_get_scheme`).
    pub fn scheme(&self) -> Option<&str> {
        self.scheme.as_deref()
    }

    /// Compare two network addresses (not in upstream public API but
    /// useful for testing).
    pub fn equal(&self, other: &NetworkAddress) -> bool {
        self.hostname == other.hostname
            && self.port == other.port
            && self.scheme == other.scheme
    }
}

// ───────────────────────────── tests ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_fields() {
        let addr = NetworkAddress::new("example.com", 80);
        assert_eq!(addr.hostname(), "example.com");
        assert_eq!(addr.port(), 80);
        assert_eq!(addr.scheme(), None);
    }

    #[test]
    fn new_loopback_uses_localhost() {
        let addr = NetworkAddress::new_loopback(8080);
        assert_eq!(addr.hostname(), "localhost");
        assert_eq!(addr.port(), 8080);
    }

    #[test]
    fn parse_plain_hostname_uses_default_port() {
        let addr = NetworkAddress::parse("example.com", 443).unwrap();
        assert_eq!(addr.hostname(), "example.com");
        assert_eq!(addr.port(), 443);
    }

    #[test]
    fn parse_host_port() {
        let addr = NetworkAddress::parse("example.com:8080", 443).unwrap();
        assert_eq!(addr.hostname(), "example.com");
        assert_eq!(addr.port(), 8080);
    }

    #[test]
    fn parse_ipv6_bracketed_with_port() {
        let addr = NetworkAddress::parse("[2001:db8::1]:888", 443).unwrap();
        assert_eq!(addr.hostname(), "2001:db8::1");
        assert_eq!(addr.port(), 888);
    }

    #[test]
    fn parse_ipv6_bracketed_no_port() {
        let addr = NetworkAddress::parse("[2001:db8::1]", 443).unwrap();
        assert_eq!(addr.hostname(), "2001:db8::1");
        assert_eq!(addr.port(), 443);
    }

    #[test]
    fn parse_ipv6_unescaped_no_port() {
        // Multiple ':' → treated as unescaped IPv6, no port.
        let addr = NetworkAddress::parse("2001:db8::1", 443).unwrap();
        assert_eq!(addr.hostname(), "2001:db8::1");
        assert_eq!(addr.port(), 443);
    }

    #[test]
    fn parse_empty_port() {
        let err = NetworkAddress::parse("example.com:", 443).unwrap_err();
        assert_eq!(err, NetworkAddressError::EmptyPort);
    }

    #[test]
    fn parse_invalid_numeric_port() {
        let err = NetworkAddress::parse("example.com:99999", 443).unwrap_err();
        assert_eq!(err, NetworkAddressError::InvalidPort);
        let err = NetworkAddress::parse("example.com:12abc", 443).unwrap_err();
        assert_eq!(err, NetworkAddressError::InvalidPort);
    }

    #[test]
    fn parse_service_name_unsupported() {
        // "http" is a service name — we don't have getservbyname.
        let err = NetworkAddress::parse("example.com:http", 443).unwrap_err();
        assert_eq!(err, NetworkAddressError::UnknownService);
    }

    #[test]
    fn parse_unclosed_bracket() {
        let err = NetworkAddress::parse("[2001:db8::1", 443).unwrap_err();
        assert_eq!(err, NetworkAddressError::UnclosedBracket);
    }

    #[test]
    fn parse_bad_bracket() {
        // ']' followed by non-':' non-empty char.
        let err = NetworkAddress::parse("[2001:db8::1]bad", 443).unwrap_err();
        assert_eq!(err, NetworkAddressError::BadBracket);
    }

    #[test]
    fn parse_uri_http() {
        let addr = NetworkAddress::parse_uri("http://example.com:8080/path", 443).unwrap();
        assert_eq!(addr.scheme(), Some("http"));
        assert_eq!(addr.hostname(), "example.com");
        assert_eq!(addr.port(), 8080);
    }

    #[test]
    fn parse_uri_no_port_uses_default() {
        let addr = NetworkAddress::parse_uri("https://example.com/path", 443).unwrap();
        assert_eq!(addr.scheme(), Some("https"));
        assert_eq!(addr.hostname(), "example.com");
        assert_eq!(addr.port(), 443);
    }

    #[test]
    fn parse_uri_invalid() {
        let err = NetworkAddress::parse_uri("not a uri", 443).unwrap_err();
        assert_eq!(err, NetworkAddressError::InvalidUri);
    }

    #[test]
    fn equal_addresses() {
        let a = NetworkAddress::new("example.com", 80);
        let b = NetworkAddress::new("example.com", 80);
        let c = NetworkAddress::new("example.com", 81);
        let d = NetworkAddress::new("other.com", 80);
        assert!(a.equal(&b));
        assert!(!a.equal(&c)); // different port
        assert!(!a.equal(&d)); // different hostname
    }

    #[test]
    fn equal_with_scheme() {
        let a = NetworkAddress::parse_uri("http://example.com:80", 443).unwrap();
        let b = NetworkAddress::parse_uri("http://example.com:80", 443).unwrap();
        let c = NetworkAddress::new("example.com", 80); // no scheme
        assert!(a.equal(&b));
        assert!(!a.equal(&c)); // a has scheme, c doesn't
    }

    #[test]
    fn clone_preserves_fields() {
        let addr = NetworkAddress::parse_uri("http://example.com:80", 443).unwrap();
        let cloned = addr.clone();
        assert!(addr.equal(&cloned));
        assert_eq!(addr.scheme(), cloned.scheme());
    }

    #[test]
    fn parse_max_port() {
        let addr = NetworkAddress::parse("example.com:65535", 443).unwrap();
        assert_eq!(addr.port(), 65535);
    }

    #[test]
    fn parse_port_zero() {
        let addr = NetworkAddress::parse("example.com:0", 443).unwrap();
        assert_eq!(addr.port(), 0);
    }
}
