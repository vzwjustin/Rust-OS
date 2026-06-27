//! GProxyResolverPortal matching `gio/gproxyresolverportal.h`.
//! Portal-based proxy resolver. In this no_std port we model it with
//! a list of proxy URIs and portal availability.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A portal-based proxy resolver (`GProxyResolverPortal`).
pub struct ProxyResolverPortal {
    proxies: Mutex<Vec<String>>,
    available: Mutex<bool>,
}

impl ProxyResolverPortal {
    pub fn new() -> Self {
        Self {
            proxies: Mutex::new(Vec::new()),
            available: Mutex::new(false),
        }
    }

    pub fn is_available(&self) -> bool {
        *self.available.lock()
    }
    pub fn set_available(&self, available: bool) {
        *self.available.lock() = available;
    }

    pub fn lookup(&self, _uri: &str) -> Vec<String> {
        if !*self.available.lock() {
            return vec!["direct://".to_string()];
        }
        if self.proxies.lock().is_empty() {
            return vec!["direct://".to_string()];
        }
        self.proxies.lock().clone()
    }

    pub fn set_proxies(&self, proxies: Vec<String>) {
        *self.proxies.lock() = proxies;
    }
}

impl Default for ProxyResolverPortal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_unavailable() {
        let r = ProxyResolverPortal::new();
        assert_eq!(
            r.lookup("http://example.com"),
            vec!["direct://".to_string()]
        );
    }

    #[test]
    fn test_lookup_with_proxies() {
        let r = ProxyResolverPortal::new();
        r.set_available(true);
        r.set_proxies(vec!["http://proxy:8080".to_string()]);
        assert_eq!(
            r.lookup("http://example.com"),
            vec!["http://proxy:8080".to_string()]
        );
    }
}
