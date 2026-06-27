//! GSimpleProxyResolver matching `gio/gsimpleproxyresolver.h` /
//! `gio/gsimpleproxyresolver.c`.
//!
//! Resolves proxy URIs from a default proxy and an ignore-host list.
//! Hosts matching any ignore pattern (prefix or suffix) connect
//! directly via `"direct://"`.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A simple proxy resolver (`GSimpleProxyResolver`).
///
/// Returns [`default_proxy`](SimpleProxyResolver::get_default_proxy) for
/// most URIs unless the URI host matches an entry in
/// [`ignore_hosts`](SimpleProxyResolver::add_ignore_host), in which case
/// `"direct://"` is returned.
pub struct SimpleProxyResolver {
    default_proxy: Mutex<String>,
    ignore_hosts: Mutex<Vec<String>>,
}

impl SimpleProxyResolver {
    /// Creates a resolver with `default_proxy` as the fallback proxy URI.
    ///
    /// Mirrors `g_simple_proxy_resolver_new`.
    pub fn new(default_proxy: &str) -> Self {
        Self {
            default_proxy: Mutex::new(default_proxy.to_string()),
            ignore_hosts: Mutex::new(Vec::new()),
        }
    }

    /// Returns `true`. Simple proxy resolution is always available.
    ///
    /// Mirrors `g_proxy_resolver_is_supported`.
    pub fn is_supported(&self) -> bool {
        true
    }

    /// Replaces the default proxy URI.
    ///
    /// Mirrors `g_simple_proxy_resolver_set_default_proxy`.
    pub fn set_default_proxy(&self, proxy: &str) {
        *self.default_proxy.lock() = proxy.to_string();
    }

    /// Returns the current default proxy URI.
    ///
    /// Mirrors `g_simple_proxy_resolver_get_default_proxy`.
    pub fn get_default_proxy(&self) -> String {
        self.default_proxy.lock().clone()
    }

    /// Adds a host pattern to the ignore list.
    ///
    /// During [`lookup`](SimpleProxyResolver::lookup), if the URI host
    /// starts with or ends with this pattern, `"direct://"` is returned.
    ///
    /// Mirrors `g_simple_proxy_resolver_set_ignore_hosts` (append form).
    pub fn add_ignore_host(&self, host: &str) {
        self.ignore_hosts.lock().push(host.to_string());
    }

    /// Returns a copy of the ignore-host patterns.
    pub fn get_ignore_hosts(&self) -> Vec<String> {
        self.ignore_hosts.lock().clone()
    }

    /// Returns the proxy URI(s) to use for `uri`.
    ///
    /// If the URI host matches any ignore pattern by prefix or suffix,
    /// returns `["direct://"]`. Otherwise returns the default proxy.
    ///
    /// Mirrors `g_proxy_resolver_lookup` on `GSimpleProxyResolver`.
    pub fn lookup(&self, uri: &str) -> Vec<String> {
        let host = extract_host(uri);
        for pattern in self.ignore_hosts.lock().iter() {
            if host_matches_ignore(host, pattern) {
                return vec!["direct://".to_string()];
            }
        }
        vec![self.default_proxy.lock().clone()]
    }
}

/// Extract the hostname portion from a URI string (no scheme, port, or path).
fn extract_host(uri: &str) -> &str {
    let after_scheme = uri.split("://").nth(1).unwrap_or(uri);
    let host_port_path = after_scheme.split('/').next().unwrap_or(after_scheme);
    let host_port = host_port_path.rsplit('@').next().unwrap_or(host_port_path);

    if let Some(bracketed) = host_port.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or(bracketed)
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    }
}

/// Returns `true` when `host` matches `pattern` by prefix or suffix.
fn host_matches_ignore(host: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    host.starts_with(pattern) || host.ends_with(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_is_supported() {
        let r = SimpleProxyResolver::new("http://proxy:8080");
        assert_eq!(r.get_default_proxy(), "http://proxy:8080");
        assert!(r.is_supported());
    }

    #[test]
    fn lookup_returns_default_proxy() {
        let r = SimpleProxyResolver::new("socks5://proxy:1080");
        assert_eq!(
            r.lookup("http://example.com/page"),
            vec!["socks5://proxy:1080"]
        );
    }

    #[test]
    fn ignore_host_prefix_match_returns_direct() {
        let r = SimpleProxyResolver::new("http://proxy:8080");
        r.add_ignore_host("local");
        assert_eq!(r.lookup("http://localhost/app"), vec!["direct://"]);
    }

    #[test]
    fn ignore_host_suffix_match_returns_direct() {
        let r = SimpleProxyResolver::new("http://proxy:8080");
        r.add_ignore_host(".internal");
        assert_eq!(r.lookup("https://db.corp.internal:5432"), vec!["direct://"]);
    }

    #[test]
    fn non_matching_host_uses_default() {
        let r = SimpleProxyResolver::new("http://proxy:8080");
        r.add_ignore_host("localhost");
        assert_eq!(r.lookup("http://example.com"), vec!["http://proxy:8080"]);
    }

    #[test]
    fn set_default_proxy_changes_lookup() {
        let r = SimpleProxyResolver::new("direct://");
        r.set_default_proxy("http://new-proxy:3128");
        assert_eq!(
            r.lookup("http://remote.test"),
            vec!["http://new-proxy:3128"]
        );
    }

    #[test]
    fn extract_host_strips_scheme_port_and_path() {
        assert_eq!(
            extract_host("https://user@host.example:443/path"),
            "host.example"
        );
        assert_eq!(extract_host("[::1]:8080"), "::1");
    }
}
