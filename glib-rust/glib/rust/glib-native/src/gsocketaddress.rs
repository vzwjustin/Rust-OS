//! GIO socket address matching `gio/gsocketaddress.h` /
//! `gio/gsocketaddress.c`.
//!
//! Upstream `GSocketAddress` is an abstract `GObject` subclass that also
//! implements `GSocketConnectable`. We port it as an `enum` wrapping the
//! concrete socket address types, since the GObject abstract class /
//! interface system is deferred (Phase 9).
//!
//! Provides:
//! - `SocketAddress` enum (`Inet` / `Unix` / `Native`).
//! - `family()`, `native_size()`, `to_native()`, `new_from_native()`.
//! - `to_string()`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::ginetaddress::SocketFamily;
use crate::ginetsocketaddress::InetSocketAddress;
use crate::gioerror::IOErrorEnum;
use crate::gunixsocketaddress::UnixSocketAddress;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

/// A socket address (`GSocketAddress`).
///
/// This is the equivalent of `struct sockaddr` and its subtypes in the
/// BSD sockets API. It is an abstract type in upstream GLib; here we
/// model it as an enum with variants for each concrete address type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SocketAddress {
    /// An IPv4 or IPv6 inet socket address (`GInetSocketAddress`).
    Inet(InetSocketAddress),
    /// A UNIX domain socket address (`GUnixSocketAddress`).
    Unix(UnixSocketAddress),
    /// A native socket address that could not be classified as inet or
    /// unix (`GNativeSocketAddress`). Stores the raw `sockaddr` bytes.
    Native(Vec<u8>),
}

impl SocketAddress {
    /// Gets the socket family of this address.
    ///
    /// Mirrors `g_socket_address_get_family`.
    pub fn family(&self) -> SocketFamily {
        match self {
            SocketAddress::Inet(sa) => sa.family(),
            SocketAddress::Unix(sa) => sa.family(),
            SocketAddress::Native(bytes) => {
                if bytes.len() >= 2 {
                    let family = u16::from_ne_bytes([bytes[0], bytes[1]]);
                    match family {
                        2 => SocketFamily::Ipv4,
                        10 => SocketFamily::Ipv6,
                        1 => SocketFamily::Unix,
                        _ => SocketFamily::Invalid,
                    }
                } else {
                    SocketFamily::Invalid
                }
            }
        }
    }

    /// Gets the size of the native `struct sockaddr` representation.
    ///
    /// Returns the size in bytes, or 0 if the address is not valid.
    /// Mirrors `g_socket_address_get_native_size`.
    pub fn native_size(&self) -> usize {
        match self {
            SocketAddress::Inet(sa) => sa.native_size(),
            SocketAddress::Unix(sa) => sa.native_size(),
            SocketAddress::Native(bytes) => bytes.len(),
        }
    }

    /// Serializes this socket address to a native `struct sockaddr` in
    /// `dest`.
    ///
    /// Returns `Ok(())` on success, or an `IOErrorEnum` on failure.
    /// Mirrors `g_socket_address_to_native`.
    pub fn to_native(&self, dest: &mut [u8]) -> Result<(), IOErrorEnum> {
        match self {
            SocketAddress::Inet(sa) => sa.to_native(dest),
            SocketAddress::Unix(sa) => sa.to_native(dest),
            SocketAddress::Native(bytes) => {
                if dest.len() < bytes.len() {
                    return Err(IOErrorEnum::NoSpace);
                }
                dest[..bytes.len()].copy_from_slice(bytes);
                Ok(())
            }
        }
    }

    /// Creates a `SocketAddress` from a native `struct sockaddr`.
    ///
    /// Attempts to classify the native address as IPv4 or IPv6 (returning
    /// an `Inet` variant). If the family is unrecognized, returns a
    /// `Native` variant with the raw bytes.
    ///
    /// Returns `None` if `native` is too small to contain a valid address.
    /// Mirrors `g_socket_address_new_from_native`.
    pub fn new_from_native(native: &[u8]) -> Option<Self> {
        if native.len() < 2 {
            return None;
        }

        // Try InetSocketAddress first (handles AF_INET and AF_INET6).
        if let Some(sa) = InetSocketAddress::from_native(native) {
            return Some(SocketAddress::Inet(sa));
        }

        // Try UnixSocketAddress (handles AF_UNIX).
        if let Some(sa) = UnixSocketAddress::from_native(native) {
            return Some(SocketAddress::Unix(sa));
        }

        // Check for AF_UNSPEC.
        let family = u16::from_ne_bytes([native[0], native[1]]);
        if family == 0 {
            return None;
        }

        // Unknown family — store as Native.
        Some(SocketAddress::Native(native.to_vec()))
    }

    /// Returns the connectable string representation.
    ///
    /// For `Inet` variants, delegates to `InetSocketAddress::to_string`.
    /// For `Unix` variants, delegates to `UnixSocketAddress::to_string`.
    /// For `Native` variants, returns a placeholder string.
    pub fn to_string(&self) -> String {
        match self {
            SocketAddress::Inet(sa) => sa.to_string(),
            SocketAddress::Unix(sa) => sa.to_string(),
            SocketAddress::Native(_) => String::from("(native)"),
        }
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ginetaddress::InetAddress;
    use crate::gunixsocketaddress::UnixSocketAddressType;

    #[test]
    fn test_inet_variant() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("192.168.1.1").unwrap(), 80);
        let addr = SocketAddress::Inet(sa);
        assert_eq!(addr.family(), SocketFamily::Ipv4);
        assert_eq!(addr.native_size(), 16);
        assert_eq!(addr.to_string(), "192.168.1.1:80");
    }

    #[test]
    fn test_inet_variant_ipv6() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("2001:db8::1").unwrap(), 443);
        let addr = SocketAddress::Inet(sa);
        assert_eq!(addr.family(), SocketFamily::Ipv6);
        assert_eq!(addr.native_size(), 28);
        assert_eq!(addr.to_string(), "[2001:db8::1]:443");
    }

    #[test]
    fn test_unix_variant() {
        let sa = UnixSocketAddress::new("/tmp/socket");
        let addr = SocketAddress::Unix(sa);
        assert_eq!(addr.family(), SocketFamily::Unix);
        assert_eq!(addr.native_size(), 110); // SockaddrUn::SIZE
        assert_eq!(addr.to_string(), "/tmp/socket");
    }

    #[test]
    fn test_unix_variant_anonymous() {
        let sa = UnixSocketAddress::new_with_type(b"", None, UnixSocketAddressType::Anonymous);
        let addr = SocketAddress::Unix(sa);
        assert_eq!(addr.family(), SocketFamily::Unix);
        assert_eq!(addr.to_string(), "anonymous");
    }

    #[test]
    fn test_new_from_native_unix() {
        let original = UnixSocketAddress::new("/var/run/socket");
        let mut buf = vec![0u8; 110];
        original.to_native(&mut buf).unwrap();
        let addr = SocketAddress::new_from_native(&buf).unwrap();
        match addr {
            SocketAddress::Unix(sa) => {
                assert_eq!(sa.path(), b"/var/run/socket");
                assert_eq!(sa.address_type(), UnixSocketAddressType::Path);
            }
            _ => panic!("expected Unix variant"),
        }
    }

    #[test]
    fn test_native_variant() {
        let raw = [0u8; 32];
        let addr = SocketAddress::Native(raw.to_vec());
        assert_eq!(addr.native_size(), 32);
        assert_eq!(addr.to_string(), "(native)");
    }

    #[test]
    fn test_native_variant_family_detection() {
        let mut raw = [0u8; 32];
        raw[0] = 2; // AF_INET
        raw[1] = 0;
        let addr = SocketAddress::Native(raw.to_vec());
        assert_eq!(addr.family(), SocketFamily::Ipv4);
    }

    #[test]
    fn test_native_variant_family_ipv6() {
        let mut raw = [0u8; 32];
        raw[0] = 10; // AF_INET6
        raw[1] = 0;
        let addr = SocketAddress::Native(raw.to_vec());
        assert_eq!(addr.family(), SocketFamily::Ipv6);
    }

    #[test]
    fn test_native_variant_family_unix() {
        let mut raw = [0u8; 32];
        raw[0] = 1; // AF_UNIX
        raw[1] = 0;
        let addr = SocketAddress::Native(raw.to_vec());
        assert_eq!(addr.family(), SocketFamily::Unix);
    }

    #[test]
    fn test_native_variant_family_unknown() {
        let mut raw = [0u8; 32];
        raw[0] = 99;
        let addr = SocketAddress::Native(raw.to_vec());
        assert_eq!(addr.family(), SocketFamily::Invalid);
    }

    #[test]
    fn test_new_from_native_ipv4() {
        let original =
            InetSocketAddress::new(InetAddress::new_from_string("10.0.0.1").unwrap(), 1234);
        let mut buf = vec![0u8; 16];
        original.to_native(&mut buf).unwrap();
        let addr = SocketAddress::new_from_native(&buf).unwrap();
        match addr {
            SocketAddress::Inet(sa) => {
                assert_eq!(sa.address().to_string(), "10.0.0.1");
                assert_eq!(sa.port(), 1234);
            }
            _ => panic!("expected Inet variant"),
        }
    }

    #[test]
    fn test_new_from_native_ipv6() {
        let original = InetSocketAddress::new(InetAddress::new_from_string("::1").unwrap(), 80);
        let mut buf = vec![0u8; 28];
        original.to_native(&mut buf).unwrap();
        let addr = SocketAddress::new_from_native(&buf).unwrap();
        match addr {
            SocketAddress::Inet(sa) => {
                assert_eq!(sa.address().to_string(), "::1");
                assert_eq!(sa.port(), 80);
            }
            _ => panic!("expected Inet variant"),
        }
    }

    #[test]
    fn test_new_from_native_unknown_family() {
        let mut buf = [0u8; 64];
        buf[0] = 99; // Unknown family
        let addr = SocketAddress::new_from_native(&buf).unwrap();
        match addr {
            SocketAddress::Native(_) => {}
            _ => panic!("expected Native variant"),
        }
    }

    #[test]
    fn test_new_from_native_af_unspec_returns_none() {
        let buf = [0u8; 64];
        assert!(SocketAddress::new_from_native(&buf).is_none());
    }

    #[test]
    fn test_new_from_native_too_small() {
        assert!(SocketAddress::new_from_native(&[0u8; 1]).is_none());
        assert!(SocketAddress::new_from_native(&[]).is_none());
    }

    #[test]
    fn test_to_native_inet() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
        let addr = SocketAddress::Inet(sa);
        let mut buf = [0u8; 16];
        addr.to_native(&mut buf).unwrap();
        // Check port in network byte order
        let port = u16::from_be_bytes([buf[2], buf[3]]);
        assert_eq!(port, 80);
    }

    #[test]
    fn test_to_native_no_space() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
        let addr = SocketAddress::Inet(sa);
        let mut buf = [0u8; 4];
        assert_eq!(addr.to_native(&mut buf), Err(IOErrorEnum::NoSpace));
    }

    #[test]
    fn test_to_native_native_variant() {
        let raw = vec![1u8, 2, 3, 4, 5];
        let addr = SocketAddress::Native(raw.clone());
        let mut buf = [0u8; 8];
        addr.to_native(&mut buf).unwrap();
        assert_eq!(&buf[..5], &raw[..]);
    }

    #[test]
    fn test_equal() {
        let sa1 = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
        let sa2 = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
        let a = SocketAddress::Inet(sa1);
        let b = SocketAddress::Inet(sa2);
        assert_eq!(a, b);
    }

    #[test]
    fn test_clone() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
        let addr = SocketAddress::Inet(sa);
        let addr2 = addr.clone();
        assert_eq!(addr, addr2);
    }
}
