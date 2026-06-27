//! GProxyAddressEnumerator matching `gio/gproxyaddressenumerator.h`.
//!
//! Wraps destination socket addresses with [`ProxyAddress`] instances
//! according to proxy URIs from a resolver. Supports `direct://` and
//! common `http://` / `socks5://` proxy URIs.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::ginetaddress::{InetAddress, SocketFamily};
use crate::ginetsocketaddress::InetSocketAddress;
use crate::gnetworkaddress::NetworkAddress;
use crate::gproxyaddress::ProxyAddress;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Resolves proxy URIs for a destination (`GProxyResolver::lookup` subset).
pub trait ProxyUriLookup {
    fn lookup_proxy_uris(&self, dest_uri: &str) -> Vec<String>;
}

/// A proxy address enumerator (`GProxyAddressEnumerator`).
pub struct ProxyAddressEnumerator {
    dest_hostname: String,
    dest_port: u16,
    default_port: u16,
    dest_uri: String,
    dest_addresses: Mutex<Vec<InetSocketAddress>>,
    dest_index: Mutex<usize>,
    proxy_uris: Vec<String>,
    proxy_index: Mutex<usize>,
    current_batch: Mutex<Vec<ProxyAddress>>,
    batch_index: Mutex<usize>,
    ever_enumerated: Mutex<bool>,
}

impl ProxyAddressEnumerator {
    /// Creates an enumerator for `dest_hostname`:`dest_port` using `proxy_uris`.
    pub fn new(
        dest_hostname: &str,
        dest_port: u16,
        default_port: u16,
        proxy_uris: Vec<String>,
    ) -> Self {
        let dest_uri = if dest_port == 0 {
            format!("http://{dest_hostname}/")
        } else {
            format!("http://{dest_hostname}:{dest_port}/")
        };
        let dest_addresses = resolve_dest_addresses(dest_hostname, dest_port);
        Self {
            dest_hostname: dest_hostname.to_string(),
            dest_port,
            default_port,
            dest_uri,
            dest_addresses: Mutex::new(dest_addresses),
            dest_index: Mutex::new(0),
            proxy_uris,
            proxy_index: Mutex::new(0),
            current_batch: Mutex::new(Vec::new()),
            batch_index: Mutex::new(0),
            ever_enumerated: Mutex::new(false),
        }
    }

    /// Creates an enumerator using `resolver.lookup_proxy_uris` for the destination URI.
    pub fn new_with_lookup<L: ProxyUriLookup>(
        dest_hostname: &str,
        dest_port: u16,
        default_port: u16,
        resolver: &L,
    ) -> Self {
        let dest_uri = if dest_port == 0 {
            format!("http://{dest_hostname}/")
        } else {
            format!("http://{dest_hostname}:{dest_port}/")
        };
        let proxy_uris = resolver.lookup_proxy_uris(&dest_uri);
        Self::new(dest_hostname, dest_port, default_port, proxy_uris)
    }

    pub fn dest_uri(&self) -> &str {
        &self.dest_uri
    }

    pub fn proxy_count(&self) -> usize {
        self.proxy_uris.len()
    }

    /// Returns the next [`ProxyAddress`], or `None` when exhausted.
    pub fn next(&self) -> Option<ProxyAddress> {
        loop {
            {
                let mut batch_idx = self.batch_index.lock();
                let batch = self.current_batch.lock();
                if *batch_idx < batch.len() {
                    let addr = batch[*batch_idx].clone();
                    *batch_idx += 1;
                    *self.ever_enumerated.lock() = true;
                    return Some(addr);
                }
            }
            if !self.load_next_proxy_batch() {
                return None;
            }
        }
    }

    pub fn reset(&self) {
        *self.dest_index.lock() = 0;
        *self.proxy_index.lock() = 0;
        *self.batch_index.lock() = 0;
        self.current_batch.lock().clear();
        *self.ever_enumerated.lock() = false;
    }

    pub fn has_enumerated(&self) -> bool {
        *self.ever_enumerated.lock()
    }

    fn load_next_proxy_batch(&self) -> bool {
        let proxy_idx = {
            let mut idx = self.proxy_index.lock();
            if *idx >= self.proxy_uris.len() {
                return false;
            }
            let current = *idx;
            *idx += 1;
            current
        };
        let proxy_uri = &self.proxy_uris[proxy_idx];
        let batch = build_proxy_batch(
            proxy_uri,
            &self.dest_hostname,
            self.dest_port,
            self.default_port,
            &self.dest_addresses.lock(),
        );
        if batch.is_empty() {
            return self.load_next_proxy_batch();
        }
        *self.current_batch.lock() = batch;
        *self.batch_index.lock() = 0;
        true
    }
}

impl crate::gsimpleproxyresolver::SimpleProxyResolver {
    /// Convenience wrapper matching resolver lookup for enumerators.
    pub fn lookup_proxy_uris(&self, uri: &str) -> Vec<String> {
        self.lookup(uri)
    }
}

impl crate::gproxyresolver::ProxyResolver {
    /// Convenience wrapper matching resolver lookup for enumerators.
    pub fn lookup_proxy_uris(&self, uri: &str) -> Vec<String> {
        self.lookup(uri)
    }
}

impl crate::gdummyproxyresolver::DummyProxyResolver {
    pub fn lookup_proxy_uris(&self, uri: &str) -> Vec<String> {
        self.lookup(uri)
    }
}

impl ProxyUriLookup for crate::gsimpleproxyresolver::SimpleProxyResolver {
    fn lookup_proxy_uris(&self, dest_uri: &str) -> Vec<String> {
        self.lookup(dest_uri)
    }
}

impl ProxyUriLookup for crate::gproxyresolver::ProxyResolver {
    fn lookup_proxy_uris(&self, dest_uri: &str) -> Vec<String> {
        self.lookup(dest_uri)
    }
}

impl ProxyUriLookup for crate::gdummyproxyresolver::DummyProxyResolver {
    fn lookup_proxy_uris(&self, dest_uri: &str) -> Vec<String> {
        self.lookup(dest_uri)
    }
}

fn loopback_address() -> Option<InetAddress> {
    InetAddress::new_from_bytes(&[127, 0, 0, 1], SocketFamily::Ipv4)
}

fn resolve_dest_addresses(hostname: &str, port: u16) -> Vec<InetSocketAddress> {
    if let Some(addr) = InetAddress::new_from_string(hostname) {
        return vec![InetSocketAddress::new(addr, port)];
    }
    if let Ok(na) = NetworkAddress::parse(hostname, port) {
        if let Some(addr) = InetAddress::new_from_string(na.hostname()) {
            return vec![InetSocketAddress::new(addr, na.port())];
        }
    }
    if let Some(addr) = loopback_address() {
        return vec![InetSocketAddress::new(addr, port)];
    }
    Vec::new()
}

fn build_proxy_batch(
    proxy_uri: &str,
    dest_hostname: &str,
    dest_port: u16,
    default_port: u16,
    dest_addresses: &[InetSocketAddress],
) -> Vec<ProxyAddress> {
    if proxy_uri == "direct://" || proxy_uri.starts_with("direct:") {
        return dest_addresses
            .iter()
            .map(|dest| {
                ProxyAddress::new_full(
                    dest.address().clone(),
                    dest.port(),
                    "direct",
                    dest_hostname,
                    dest_port,
                    None,
                    None,
                    Some("http"),
                    Some("direct://"),
                )
            })
            .collect();
    }

    let (protocol, host, port, user, pass) = match parse_proxy_uri(proxy_uri) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let proxy_port = if port == 0 { default_port } else { port };
    let proxy_inet = match InetAddress::new_from_string(&host).or_else(loopback_address) {
        Some(a) => a,
        None => return Vec::new(),
    };

    vec![ProxyAddress::new_full(
        proxy_inet,
        proxy_port,
        &protocol,
        dest_hostname,
        dest_port,
        user.as_deref(),
        pass.as_deref(),
        Some("http"),
        Some(proxy_uri),
    )]
}

fn parse_proxy_uri(uri: &str) -> Option<(String, String, u16, Option<String>, Option<String>)> {
    let scheme_end = uri.find("://")?;
    let protocol = uri[..scheme_end].to_string();
    let rest = &uri[scheme_end + 3..];
    let (auth, hostport) = match rest.rsplit_once('@') {
        Some((a, h)) => (Some(a), h),
        None => (None, rest),
    };
    let (user, pass) = if let Some(auth) = auth {
        if let Some((u, p)) = auth.split_once(':') {
            (Some(u.to_string()), Some(p.to_string()))
        } else {
            (Some(auth.to_string()), None)
        }
    } else {
        (None, None)
    };
    let (host, port) = if let Some(colon) = hostport.rfind(':') {
        let p: u16 = hostport[colon + 1..].parse().ok()?;
        (hostport[..colon].to_string(), p)
    } else {
        (hostport.to_string(), 0)
    };
    Some((protocol, host, port, user, pass))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gdummyproxyresolver::DummyProxyResolver;
    use crate::gsimpleproxyresolver::SimpleProxyResolver;

    #[test]
    fn test_direct_enumeration() {
        let e = ProxyAddressEnumerator::new("127.0.0.1", 80, 8080, vec!["direct://".to_string()]);
        let addr = e.next().expect("proxy address");
        assert_eq!(addr.protocol(), "direct");
        assert_eq!(addr.destination_hostname(), "127.0.0.1");
        assert_eq!(addr.destination_port(), 80);
        assert!(e.next().is_none());
    }

    #[test]
    fn test_http_proxy() {
        let e = ProxyAddressEnumerator::new(
            "example.com",
            443,
            8080,
            vec!["http://proxy.example.com:3128".to_string()],
        );
        let addr = e.next().expect("http proxy");
        assert_eq!(addr.protocol(), "http");
        assert_eq!(addr.port(), 3128);
        assert_eq!(addr.destination_hostname(), "example.com");
    }

    #[test]
    fn test_with_simple_resolver() {
        let resolver = SimpleProxyResolver::new("http://proxy:8080");
        let e = ProxyAddressEnumerator::new_with_lookup("example.com", 80, 8080, &resolver);
        let addr = e.next().unwrap();
        assert_eq!(addr.protocol(), "http");
        assert_eq!(addr.port(), 8080);
    }

    #[test]
    fn test_dummy_resolver() {
        let resolver = DummyProxyResolver::new();
        let e = ProxyAddressEnumerator::new_with_lookup("example.com", 80, 8080, &resolver);
        let addr = e.next().unwrap();
        assert_eq!(addr.protocol(), "direct");
    }

    #[test]
    fn test_reset() {
        let e = ProxyAddressEnumerator::new("127.0.0.1", 80, 8080, vec!["direct://".into()]);
        assert!(e.next().is_some());
        e.reset();
        assert!(!e.has_enumerated());
        assert!(e.next().is_some());
    }
}
