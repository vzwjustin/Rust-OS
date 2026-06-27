//! GIO `GProxyResolver` — maps URIs to proxy URIs.
//!
//! This is a no_std port of `gio/gproxyresolver.h`. A [`ProxyResolver`] holds
//! a configurable table of (uri_prefix, proxy_uri) pairs and a fallback
//! "default proxy". Calling [`ProxyResolver::lookup`] walks the table for the
//! first matching prefix; if none matches it returns the default proxy.

use crate::prelude::*;
use spin::Mutex;

/// Maps URIs to proxy URIs.
///
/// Mirrors `GProxyResolver` from GIO. In this no_std port the resolver is
/// purely in-memory; no system proxy settings are consulted.
pub struct ProxyResolver {
    /// Ordered list of `(uri_prefix, proxy_uri)` pairs.
    proxies: Mutex<Vec<(String, String)>>,
    /// Fallback proxy returned when no prefix matches.
    default_proxy: Mutex<String>,
}

impl ProxyResolver {
    /// Creates a new `ProxyResolver` with `"direct://"` as the default proxy.
    pub fn new() -> Self {
        Self {
            proxies: Mutex::new(Vec::new()),
            default_proxy: Mutex::new(String::from("direct://")),
        }
    }

    /// Alias for [`new`](ProxyResolver::new); mirrors
    /// `g_proxy_resolver_get_default`.
    pub fn new_default() -> Self {
        Self::new()
    }

    /// Returns `true`. This port always supports proxy resolution.
    ///
    /// Mirrors `g_proxy_resolver_is_supported`.
    pub fn is_supported(&self) -> bool {
        true
    }

    /// Registers `proxy_uri` as the proxy for URIs whose string representation
    /// starts with `uri_prefix`.
    ///
    /// Later registrations for the same prefix *append* a second entry; the
    /// first matching entry wins in [`lookup`](ProxyResolver::lookup).
    pub fn add_proxy(&self, uri_prefix: &str, proxy_uri: &str) {
        self.proxies
            .lock()
            .push((String::from(uri_prefix), String::from(proxy_uri)));
    }

    /// Returns the proxy URI(s) to use for `uri`.
    ///
    /// Walks the registered (prefix, proxy) pairs in insertion order and
    /// returns the first matching proxy URI. If no prefix matches, the default
    /// proxy is returned. The result always contains at least one entry.
    ///
    /// Mirrors `g_proxy_resolver_lookup`.
    pub fn lookup(&self, uri: &str) -> Vec<String> {
        let proxies = self.proxies.lock();
        for (prefix, proxy_uri) in proxies.iter() {
            if uri.starts_with(prefix.as_str()) {
                return vec![proxy_uri.clone()];
            }
        }
        vec![self.default_proxy.lock().clone()]
    }

    /// Replaces the fallback proxy with `proxy_uri`.
    pub fn set_default_proxy(&self, proxy_uri: &str) {
        *self.default_proxy.lock() = String::from(proxy_uri);
    }

    /// Returns the current fallback proxy URI.
    pub fn get_default_proxy(&self) -> String {
        self.default_proxy.lock().clone()
    }
}

impl Default for ProxyResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_supported() {
        let r = ProxyResolver::new();
        assert!(r.is_supported());
    }

    #[test]
    fn default_proxy_is_direct() {
        let r = ProxyResolver::new();
        assert_eq!(r.get_default_proxy(), "direct://");
    }

    #[test]
    fn lookup_empty_returns_default() {
        let r = ProxyResolver::new();
        let result = r.lookup("https://example.com/path");
        assert_eq!(result, vec!["direct://"]);
    }

    #[test]
    fn lookup_matches_prefix() {
        let r = ProxyResolver::new();
        r.add_proxy("https://", "https://proxy.example.com:8080");
        let result = r.lookup("https://example.com/page");
        assert_eq!(result, vec!["https://proxy.example.com:8080"]);
    }

    #[test]
    fn lookup_non_matching_prefix_falls_back_to_default() {
        let r = ProxyResolver::new();
        r.add_proxy("ftp://", "socks5://proxy.example.com:1080");
        let result = r.lookup("https://example.com");
        assert_eq!(result, vec!["direct://"]);
    }

    #[test]
    fn lookup_first_matching_prefix_wins() {
        let r = ProxyResolver::new();
        r.add_proxy("https://", "https://first.proxy:8080");
        r.add_proxy("https://", "https://second.proxy:8080");
        let result = r.lookup("https://example.com");
        assert_eq!(result, vec!["https://first.proxy:8080"]);
    }

    #[test]
    fn set_default_proxy_changes_fallback() {
        let r = ProxyResolver::new();
        r.set_default_proxy("socks5://socks.proxy:1080");
        assert_eq!(r.get_default_proxy(), "socks5://socks.proxy:1080");
        let result = r.lookup("https://example.com");
        assert_eq!(result, vec!["socks5://socks.proxy:1080"]);
    }

    #[test]
    fn new_default_alias_works() {
        let r = ProxyResolver::new_default();
        assert_eq!(r.get_default_proxy(), "direct://");
        assert!(r.is_supported());
    }

    #[test]
    fn lookup_result_always_has_at_least_one_entry() {
        let r = ProxyResolver::new();
        r.add_proxy("ftp://", "ftp-proxy://ftp.proxy:21");
        // URI that does NOT match any prefix
        let result = r.lookup("http://unmatched.example.com");
        assert!(!result.is_empty());
    }
}
