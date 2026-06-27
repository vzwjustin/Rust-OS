//! GDummyProxyResolver matching `gio/gdummyproxyresolver.h` /
//! `gio/gdummyproxyresolver.c`.
//!
//! A no-op proxy resolver that always instructs callers to connect
//! directly. Mirrors `GDummyProxyResolver` from GIO.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// A dummy proxy resolver (`GDummyProxyResolver`).
///
/// Always returns `"direct://"` from [`lookup`](DummyProxyResolver::lookup),
/// indicating that no proxy should be used.
pub struct DummyProxyResolver;

impl DummyProxyResolver {
    /// Creates a new dummy proxy resolver.
    ///
    /// Mirrors `g_dummy_proxy_resolver_new`.
    pub fn new() -> Self {
        Self
    }

    /// Returns the process-wide default dummy resolver.
    ///
    /// Mirrors `g_dummy_proxy_resolver_get_default`.
    pub fn get_default() -> Self {
        Self::new()
    }

    /// Returns `true`. Dummy proxy resolution is always available.
    ///
    /// Mirrors `g_proxy_resolver_is_supported`.
    pub fn is_supported(&self) -> bool {
        true
    }

    /// Returns the proxy URI(s) to use for `uri`.
    ///
    /// Always returns a single-element list containing `"direct://"`.
    ///
    /// Mirrors `g_proxy_resolver_lookup` on `GDummyProxyResolver`.
    pub fn lookup(&self, _uri: &str) -> Vec<String> {
        vec!["direct://".to_string()]
    }
}

impl Default for DummyProxyResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_default_is_supported() {
        let r = DummyProxyResolver::get_default();
        assert!(r.is_supported());
    }

    #[test]
    fn lookup_always_returns_direct() {
        let r = DummyProxyResolver::new();
        assert_eq!(r.lookup("http://example.com"), vec!["direct://"]);
        assert_eq!(
            r.lookup("https://secure.example.org:8443/path"),
            vec!["direct://"]
        );
        assert_eq!(r.lookup("ftp://files.example.net"), vec!["direct://"]);
    }

    #[test]
    fn lookup_result_has_one_entry() {
        let r = DummyProxyResolver::get_default();
        let proxies = r.lookup("socks://ignored");
        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0], "direct://");
    }
}
