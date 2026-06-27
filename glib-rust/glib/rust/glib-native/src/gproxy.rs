//! GProxy matching `gio/gproxy.h`.
//!
//! Interface for proxy connection handling. In this no_std port we
//! model the trait + a simple registry for protocol→proxy mapping.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A proxy interface (`GProxy`).
pub trait Proxy {
    /// Returns the protocol this proxy handles (e.g. "http", "socks5").
    fn get_protocol(&self) -> &str;

    /// Returns whether the proxy supports hostname lookups.
    ///
    /// Mirrors `g_proxy_supports_hostname`.
    fn supports_hostname(&self) -> bool;
}

/// A simple HTTP proxy implementation.
pub struct HttpProxy {
    hostname_support: bool,
}

impl HttpProxy {
    pub fn new() -> Self {
        Self {
            hostname_support: true,
        }
    }
}

impl Proxy for HttpProxy {
    fn get_protocol(&self) -> &str {
        "http"
    }
    fn supports_hostname(&self) -> bool {
        self.hostname_support
    }
}

impl Default for HttpProxy {
    fn default() -> Self {
        Self::new()
    }
}

/// A SOCKS5 proxy implementation.
pub struct Socks5Proxy;

impl Socks5Proxy {
    pub fn new() -> Self {
        Self
    }
}

impl Proxy for Socks5Proxy {
    fn get_protocol(&self) -> &str {
        "socks5"
    }
    fn supports_hostname(&self) -> bool {
        true
    }
}

impl Default for Socks5Proxy {
    fn default() -> Self {
        Self::new()
    }
}

/// A no-op (direct) proxy that passes connections through unchanged.
pub struct DirectProxy;

impl DirectProxy {
    pub fn new() -> Self {
        Self
    }
}

impl Proxy for DirectProxy {
    fn get_protocol(&self) -> &str {
        "direct"
    }
    fn supports_hostname(&self) -> bool {
        false
    }
}

impl Default for DirectProxy {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry of proxies by protocol name.
static PROXY_REGISTRY: Mutex<Vec<(&'static str, &'static str)>> = Mutex::new(Vec::new());

/// Registers a proxy for a protocol.
pub fn register_proxy(protocol: &'static str, proxy_name: &'static str) {
    PROXY_REGISTRY.lock().push((protocol, proxy_name));
}

/// Gets the default proxy for a protocol.
///
/// Mirrors `g_proxy_get_default_for_protocol`.
pub fn get_default_for_protocol(protocol: &str) -> Option<&'static str> {
    PROXY_REGISTRY
        .lock()
        .iter()
        .find(|(p, _)| *p == protocol)
        .map(|(_, name)| *name)
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_proxy() {
        let proxy = HttpProxy::new();
        assert_eq!(proxy.get_protocol(), "http");
        assert!(proxy.supports_hostname());
    }

    #[test]
    fn test_socks5_proxy() {
        let proxy = Socks5Proxy::new();
        assert_eq!(proxy.get_protocol(), "socks5");
        assert!(proxy.supports_hostname());
    }

    #[test]
    fn test_direct_proxy() {
        let proxy = DirectProxy::new();
        assert_eq!(proxy.get_protocol(), "direct");
        assert!(!proxy.supports_hostname());
    }

    #[test]
    fn test_register_and_lookup() {
        register_proxy("test-proto", "TestProxy");
        let result = get_default_for_protocol("test-proto");
        assert_eq!(result, Some("TestProxy"));
    }

    #[test]
    fn test_lookup_missing() {
        assert!(get_default_for_protocol("nonexistent").is_none());
    }
}
