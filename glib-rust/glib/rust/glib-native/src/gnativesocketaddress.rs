//! GNativeSocketAddress matching `gio/gnativesocketaddress.h` /
//! `gio/gnativesocketaddress.c`.
//!
//! Wraps a native `sockaddr` structure as raw bytes plus a
//! [`SocketFamily`](crate::ginetaddress::SocketFamily). On bare-metal
//! targets the bytes are stored verbatim for later use by socket code.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::ginetaddress::SocketFamily;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// A native socket address (`GNativeSocketAddress`).
///
/// Holds the address family and the raw `sockaddr` bytes (e.g. from
/// `struct sockaddr_in` or `struct sockaddr_un`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NativeSocketAddress {
    family: SocketFamily,
    native_data: Vec<u8>,
}

impl NativeSocketAddress {
    /// Creates a native socket address from `family` and raw `data`.
    ///
    /// Returns `None` when `data` is empty (invalid sockaddr).
    ///
    /// Mirrors `g_native_socket_address_new`.
    pub fn new(family: SocketFamily, data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }
        Some(Self {
            family,
            native_data: data.to_vec(),
        })
    }

    /// Address family (`g_socket_address_get_family`).
    pub fn get_family(&self) -> SocketFamily {
        self.family
    }

    /// Size of the native sockaddr blob (`g_native_socket_address_get_native_size`).
    pub fn get_native_size(&self) -> usize {
        self.native_data.len()
    }

    /// Raw native sockaddr bytes (`g_native_socket_address_get_native_data`).
    pub fn get_native_data(&self) -> &[u8] {
        &self.native_data
    }

    /// Owned copy of the native sockaddr bytes.
    pub fn to_native_data(&self) -> Vec<u8> {
        self.native_data.clone()
    }

    /// Human-readable representation for logging.
    ///
    /// Mirrors `g_socket_connectable_to_string` (best-effort on bare metal).
    pub fn to_string(&self) -> String {
        let hex: String = self
            .native_data
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("native:{family:?}:{hex}", family = self.family)
    }

    /// Compares two native addresses (`g_socket_address_equal`).
    pub fn equal(&self, other: &NativeSocketAddress) -> bool {
        self.family == other.family && self.native_data == other.native_data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty_data() {
        assert!(NativeSocketAddress::new(SocketFamily::Ipv4, &[]).is_none());
    }

    #[test]
    fn new_stores_family_and_bytes() {
        let raw = [0x02, 0x00, 0x1f, 0x90, 127, 0, 0, 1];
        let addr = NativeSocketAddress::new(SocketFamily::Ipv4, &raw).unwrap();
        assert_eq!(addr.get_family(), SocketFamily::Ipv4);
        assert_eq!(addr.get_native_size(), 8);
        assert_eq!(addr.get_native_data(), &raw);
    }

    #[test]
    fn equal_compares_family_and_data() {
        let a = NativeSocketAddress::new(SocketFamily::Ipv4, &[1, 2, 3]).unwrap();
        let b = NativeSocketAddress::new(SocketFamily::Ipv4, &[1, 2, 3]).unwrap();
        let c = NativeSocketAddress::new(SocketFamily::Ipv6, &[1, 2, 3]).unwrap();
        assert!(a.equal(&b));
        assert!(!a.equal(&c));
    }

    #[test]
    fn to_string_contains_family_and_hex() {
        let addr = NativeSocketAddress::new(SocketFamily::Unix, &[1, b'a']).unwrap();
        let s = addr.to_string();
        assert!(s.contains("Unix"));
        assert!(s.contains("0161"));
    }
}
