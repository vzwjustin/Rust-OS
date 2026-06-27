//! GSocketConnectable and GSocketAddressEnumerator matching
//! `gio/gsocketconnectable.h` and `gio/gsocketaddressenumerator.h`.
//!
//! `GSocketConnectable` is an interface for objects that can enumerate
//! socket addresses. `GSocketAddressEnumerator` provides sequential
//! access to those addresses.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginetsocketaddress::InetSocketAddress;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Trait for objects that can enumerate socket addresses (`GSocketConnectable`).
pub trait SocketConnectable {
    /// Creates an address enumerator.
    fn enumerate(&self) -> SocketAddressEnumerator;

    /// Creates a proxy address enumerator.
    fn proxy_enumerate(&self) -> SocketAddressEnumerator;

    /// Formats the connectable as a string.
    fn to_string(&self) -> String;
}

/// A socket address enumerator (`GSocketAddressEnumerator`).
pub struct SocketAddressEnumerator {
    addresses: Mutex<Vec<InetSocketAddress>>,
    index: Mutex<usize>,
}

impl SocketAddressEnumerator {
    /// Creates a new enumerator from a list of addresses.
    pub fn new(addresses: Vec<InetSocketAddress>) -> Self {
        Self {
            addresses: Mutex::new(addresses),
            index: Mutex::new(0),
        }
    }

    /// Creates an empty enumerator.
    pub fn empty() -> Self {
        Self {
            addresses: Mutex::new(Vec::new()),
            index: Mutex::new(0),
        }
    }

    /// Returns the next socket address, or `None` if exhausted.
    ///
    /// Mirrors `g_socket_address_enumerator_next`.
    pub fn next(
        &self,
        _cancellable: Option<&GCancellable>,
    ) -> Result<Option<InetSocketAddress>, Error> {
        let mut idx = self.index.lock();
        let addresses = self.addresses.lock();
        if *idx >= addresses.len() {
            return Ok(None);
        }
        let addr = addresses[*idx].clone();
        *idx += 1;
        Ok(Some(addr))
    }

    /// Resets the enumerator to the beginning.
    pub fn reset(&self) {
        *self.index.lock() = 0;
    }
}

/// A simple connectable wrapping a hostname + port (`GNetworkAddress`-like).
pub struct SimpleConnectable {
    hostname: String,
    port: u16,
    addresses: Vec<InetSocketAddress>,
}

impl SimpleConnectable {
    pub fn new(hostname: &str, port: u16, addresses: Vec<InetSocketAddress>) -> Self {
        Self {
            hostname: hostname.to_string(),
            port,
            addresses,
        }
    }
}

impl SocketConnectable for SimpleConnectable {
    fn enumerate(&self) -> SocketAddressEnumerator {
        SocketAddressEnumerator::new(self.addresses.clone())
    }

    fn proxy_enumerate(&self) -> SocketAddressEnumerator {
        SocketAddressEnumerator::new(self.addresses.clone())
    }

    fn to_string(&self) -> String {
        format!("{}:{}", self.hostname, self.port)
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ginetaddress::{InetAddress, SocketFamily};

    fn make_addr(octets: [u8; 4], port: u16) -> InetSocketAddress {
        let addr = InetAddress::new_from_bytes(&octets, SocketFamily::Ipv4).unwrap();
        InetSocketAddress::new(addr, port)
    }

    #[test]
    fn test_enumerator_new() {
        let enumerator = SocketAddressEnumerator::new(vec![]);
        assert!(enumerator.next(None).unwrap().is_none());
    }

    #[test]
    fn test_enumerator_next() {
        let addr1 = make_addr([127, 0, 0, 1], 80);
        let addr2 = make_addr([192, 168, 1, 1], 443);
        let enumerator = SocketAddressEnumerator::new(vec![addr1.clone(), addr2.clone()]);
        let a1 = enumerator.next(None).unwrap().unwrap();
        assert_eq!(a1.port(), 80);
        let a2 = enumerator.next(None).unwrap().unwrap();
        assert_eq!(a2.port(), 443);
        assert!(enumerator.next(None).unwrap().is_none());
    }

    #[test]
    fn test_enumerator_reset() {
        let addr = make_addr([10, 0, 0, 1], 8080);
        let enumerator = SocketAddressEnumerator::new(vec![addr]);
        enumerator.next(None).unwrap();
        assert!(enumerator.next(None).unwrap().is_none());
        enumerator.reset();
        assert!(enumerator.next(None).unwrap().is_some());
    }

    #[test]
    fn test_simple_connectable_to_string() {
        let connectable = SimpleConnectable::new("example.com", 443, vec![]);
        assert_eq!(connectable.to_string(), "example.com:443");
    }

    #[test]
    fn test_simple_connectable_enumerate() {
        let addr = make_addr([127, 0, 0, 1], 80);
        let connectable = SimpleConnectable::new("localhost", 80, vec![addr]);
        let enumerator = connectable.enumerate();
        let a = enumerator.next(None).unwrap().unwrap();
        assert_eq!(a.port(), 80);
    }

    #[test]
    fn test_simple_connectable_proxy_enumerate() {
        let addr = make_addr([127, 0, 0, 1], 8080);
        let connectable = SimpleConnectable::new("proxy.local", 8080, vec![addr]);
        let enumerator = connectable.proxy_enumerate();
        assert!(enumerator.next(None).unwrap().is_some());
    }
}
