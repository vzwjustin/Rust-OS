//! GThreadedResolver matching `gio/gthreadedresolver.h` /
//! `gio/gthreadedresolver.c`.
//!
//! Upstream `GThreadedResolver` offloads blocking `getaddrinfo` calls to a
//! worker thread pool. On bare-metal / `no_std` targets there is no libc
//! resolver and no thread pool, so this port wraps a synchronous in-memory
//! hostname table instead. The API matches [`Resolver`](crate::gresolver::Resolver)
//! for testability.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginetaddress::InetAddress;
use crate::gresolver::{Resolver, ResolverError};
use crate::gsrvtarget::SrvTarget;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

fn not_found_error() -> Error {
    Error::new(
        ResolverError::resolver_error_quark(),
        ResolverError::NotFound.to_code(),
        "name not found",
    )
}

fn service_key(service: &str, protocol: &str, domain: &str) -> String {
    format!("_{service}._{protocol}.{domain}")
}

/// A threaded DNS resolver (`GThreadedResolver`).
///
/// Maintains forward (hostname → addresses), reverse (address → hostname),
/// and SRV service tables in memory. No background threads are spawned in
/// this port; lookups are synchronous.
pub struct ThreadedResolver {
    forward: Mutex<BTreeMap<String, Vec<InetAddress>>>,
    reverse: Mutex<BTreeMap<String, String>>,
    services: Mutex<BTreeMap<String, Vec<SrvTarget>>>,
}

impl ThreadedResolver {
    /// Creates an empty resolver.
    ///
    /// Mirrors `g_threaded_resolver_new`.
    pub fn new() -> Self {
        Self {
            forward: Mutex::new(BTreeMap::new()),
            reverse: Mutex::new(BTreeMap::new()),
            services: Mutex::new(BTreeMap::new()),
        }
    }

    /// Inserts or replaces a hostname → addresses mapping.
    ///
    /// Also updates the reverse table. Intended for unit tests and
    /// embedded name-service configuration.
    pub fn add_record(&self, hostname: &str, addresses: Vec<InetAddress>) {
        let host_key = hostname.to_ascii_lowercase();
        for addr in &addresses {
            self.reverse
                .lock()
                .insert(addr.to_string(), host_key.clone());
        }
        self.forward.lock().insert(host_key, addresses);
    }

    /// Removes a hostname record and its reverse entries.
    pub fn remove_record(&self, hostname: &str) {
        let host_key = hostname.to_ascii_lowercase();
        if let Some(addresses) = self.forward.lock().remove(&host_key) {
            let mut reverse = self.reverse.lock();
            for addr in &addresses {
                reverse.remove(&addr.to_string());
            }
        }
    }

    /// Registers SRV targets for `_service._protocol.domain`.
    pub fn add_service_record(
        &self,
        service: &str,
        protocol: &str,
        domain: &str,
        targets: Vec<SrvTarget>,
    ) {
        let key = service_key(service, protocol, domain);
        self.services.lock().insert(key, targets);
    }

    /// Removes an SRV service record.
    pub fn remove_service_record(&self, service: &str, protocol: &str, domain: &str) {
        let key = service_key(service, protocol, domain);
        self.services.lock().remove(&key);
    }

    /// Returns the number of hostname records.
    pub fn record_count(&self) -> usize {
        self.forward.lock().len()
    }
}

impl Default for ThreadedResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Resolver for ThreadedResolver {
    fn lookup_by_name(
        &self,
        hostname: &str,
        _cancellable: Option<&GCancellable>,
    ) -> Result<Vec<InetAddress>, Error> {
        let key = hostname.to_ascii_lowercase();
        self.forward
            .lock()
            .get(&key)
            .cloned()
            .ok_or_else(not_found_error)
    }

    fn lookup_by_address(
        &self,
        address: &InetAddress,
        _cancellable: Option<&GCancellable>,
    ) -> Result<String, Error> {
        self.reverse
            .lock()
            .get(&address.to_string())
            .cloned()
            .ok_or_else(not_found_error)
    }

    fn lookup_service(
        &self,
        service: &str,
        protocol: &str,
        domain: &str,
        _cancellable: Option<&GCancellable>,
    ) -> Result<Vec<SrvTarget>, Error> {
        let key = service_key(service, protocol, domain);
        self.services
            .lock()
            .get(&key)
            .cloned()
            .ok_or_else(not_found_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ginetaddress::SocketFamily;

    fn ipv4(s: &str) -> InetAddress {
        InetAddress::new_from_string(s).unwrap()
    }

    #[test]
    fn lookup_by_name_found() {
        let r = ThreadedResolver::new();
        r.add_record("example.com", vec![ipv4("93.184.216.34")]);
        let addrs = r.lookup_by_name("example.com", None).unwrap();
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].to_string(), "93.184.216.34");
    }

    #[test]
    fn lookup_by_name_case_insensitive() {
        let r = ThreadedResolver::new();
        r.add_record("Example.COM", vec![ipv4("1.2.3.4")]);
        assert!(r.lookup_by_name("example.com", None).is_ok());
    }

    #[test]
    fn lookup_by_name_not_found() {
        let r = ThreadedResolver::new();
        assert!(r.lookup_by_name("missing.test", None).is_err());
    }

    #[test]
    fn lookup_by_address_reverse() {
        let r = ThreadedResolver::new();
        let addr = ipv4("10.0.0.1");
        r.add_record("host.local", vec![addr.clone()]);
        assert_eq!(r.lookup_by_address(&addr, None).unwrap(), "host.local");
    }

    #[test]
    fn remove_record_clears_forward_and_reverse() {
        let r = ThreadedResolver::new();
        let addr = ipv4("192.168.1.1");
        r.add_record("router.lan", vec![addr.clone()]);
        r.remove_record("router.lan");
        assert_eq!(r.record_count(), 0);
        assert!(r.lookup_by_name("router.lan", None).is_err());
        assert!(r.lookup_by_address(&addr, None).is_err());
    }

    #[test]
    fn lookup_service_returns_srv_targets() {
        let r = ThreadedResolver::new();
        let targets = vec![SrvTarget::new("mail.example.com", 25, 10, 0)];
        r.add_service_record("smtp", "tcp", "example.com", targets.clone());
        let result = r
            .lookup_service("smtp", "tcp", "example.com", None)
            .unwrap();
        assert_eq!(result, targets);
    }

    #[test]
    fn lookup_service_not_found() {
        let r = ThreadedResolver::new();
        assert!(r
            .lookup_service("ldap", "tcp", "example.com", None)
            .is_err());
    }

    #[test]
    fn loopback_lookup() {
        let r = ThreadedResolver::new();
        let addr = InetAddress::new_loopback(SocketFamily::Ipv4).unwrap();
        r.add_record("localhost", vec![addr]);
        assert!(r.lookup_by_name("localhost", None).is_ok());
    }
}
