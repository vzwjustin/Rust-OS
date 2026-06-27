//! GIO inet socket address matching `gio/ginetsocketaddress.h` /
//! `gio/ginetsocketaddress.c`.
//!
//! Upstream `GInetSocketAddress` is a `GObject` subclass that also
//! implements `GSocketConnectable`. We port it as a plain `pub struct`
//! with the same API rather than a registered GObject subclass,
//! mirroring upstream semantics with idiomatic Rust.
//!
//! Provides:
//! - `InetSocketAddress` struct (address + port + flowinfo + scope_id).
//! - `new(address, port)`, `new_from_string(address, port)`.
//! - `address()`, `port()`, `flowinfo()`, `scope_id()`, `family()`.
//! - `native_size()`, `to_native()` — serialize to `sockaddr_in` /
//!   `sockaddr_in6` raw bytes.
//! - `to_string()` — connectable string representation.
//! - `equal()`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::endian::{g_htons, g_ntohs};
use crate::ginetaddress::{InetAddress, SocketFamily};
use crate::prelude::*;
use alloc::format;
use alloc::string::String;

// ──────────────────── sockaddr structs (repr(C)) ──────────────────────────

/// IPv4 `struct sockaddr_in` (16 bytes on Linux).
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SockaddrIn {
    /// Address family (`AF_INET = 2`).
    pub sin_family: u16,
    /// Port in network byte order.
    pub sin_port: u16,
    /// IPv4 address in network byte order.
    pub sin_addr: [u8; 4],
    /// Padding to 16 bytes.
    pub sin_zero: [u8; 8],
}

impl SockaddrIn {
    /// Size of `struct sockaddr_in`.
    pub const SIZE: usize = 16;
}

/// IPv6 `struct sockaddr_in6` (28 bytes on Linux).
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SockaddrIn6 {
    /// Address family (`AF_INET6 = 10`).
    pub sin6_family: u16,
    /// Port in network byte order.
    pub sin6_port: u16,
    /// Flow label (network byte order).
    pub sin6_flowinfo: u32,
    /// IPv6 address (128 bits).
    pub sin6_addr: [u8; 16],
    /// Scope ID.
    pub sin6_scope_id: u32,
}

impl SockaddrIn6 {
    /// Size of `struct sockaddr_in6`.
    pub const SIZE: usize = 28;
}

// ────────────────────── InetSocketAddress ─────────────────────────────────

/// An IPv4 or IPv6 socket address — the combination of an
/// [`InetAddress`] and a port number (`GInetSocketAddress`).
///
/// Corresponds to `struct sockaddr_in` or `struct sockaddr_in6` in the
/// BSD sockets API. Plain struct port of the upstream GObject subclass.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InetSocketAddress {
    /// The IP address.
    address: InetAddress,
    /// The port number (host byte order).
    port: u16,
    /// IPv6 flowinfo (0 for IPv4).
    flowinfo: u32,
    /// IPv6 scope ID (0 for IPv4).
    scope_id: u32,
}

impl InetSocketAddress {
    /// Creates a new `InetSocketAddress` for `address` and `port`.
    ///
    /// Mirrors `g_inet_socket_address_new`.
    pub fn new(address: InetAddress, port: u16) -> Self {
        InetSocketAddress {
            address,
            port,
            flowinfo: 0,
            scope_id: 0,
        }
    }

    /// Creates a new `InetSocketAddress` from a string-form IP address
    /// and a port number.
    ///
    /// Returns `None` if `address` cannot be parsed.
    /// Mirrors `g_inet_socket_address_new_from_string`.
    pub fn new_from_string(address: &str, port: u16) -> Option<Self> {
        let addr = InetAddress::new_from_string(address)?;
        Some(InetSocketAddress::new(addr, port))
    }

    /// Creates a new `InetSocketAddress` with IPv6 flowinfo and scope_id.
    ///
    /// The `flowinfo` and `scope_id` fields are only meaningful for IPv6
    /// addresses; they are ignored for IPv4.
    pub fn new_with_ipv6_info(
        address: InetAddress,
        port: u16,
        flowinfo: u32,
        scope_id: u32,
    ) -> Self {
        InetSocketAddress {
            address,
            port,
            flowinfo,
            scope_id,
        }
    }

    /// Gets the `InetAddress` for this socket address.
    ///
    /// Mirrors `g_inet_socket_address_get_address`.
    pub fn address(&self) -> &InetAddress {
        &self.address
    }

    /// Gets the port number (host byte order).
    ///
    /// Mirrors `g_inet_socket_address_get_port`.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Gets the `sin6_flowinfo` field (IPv6 only).
    ///
    /// Returns 0 for IPv4 addresses.
    /// Mirrors `g_inet_socket_address_get_flowinfo`.
    pub fn flowinfo(&self) -> u32 {
        if self.address.family() != SocketFamily::Ipv6 {
            return 0;
        }
        self.flowinfo
    }

    /// Gets the `sin6_scope_id` field (IPv6 only).
    ///
    /// Returns 0 for IPv4 addresses.
    /// Mirrors `g_inet_socket_address_get_scope_id`.
    pub fn scope_id(&self) -> u32 {
        if self.address.family() != SocketFamily::Ipv6 {
            return 0;
        }
        self.scope_id
    }

    /// Gets the socket family of this address.
    ///
    /// Mirrors `g_socket_address_get_family` (delegates to `InetAddress`).
    pub fn family(&self) -> SocketFamily {
        self.address.family()
    }

    /// Gets the size of the native `struct sockaddr` representation.
    ///
    /// Returns `SockaddrIn::SIZE` for IPv4, `SockaddrIn6::SIZE` for IPv6.
    /// Mirrors `g_inet_socket_address_get_native_size`.
    pub fn native_size(&self) -> usize {
        match self.address.family() {
            SocketFamily::Ipv4 => SockaddrIn::SIZE,
            SocketFamily::Ipv6 => SockaddrIn6::SIZE,
            _ => 0,
        }
    }

    /// Serializes this socket address to a native `struct sockaddr_in`
    /// or `struct sockaddr_in6` in `dest`.
    ///
    /// Returns `Ok(())` on success, or an `IOErrorEnum` on failure.
    /// Mirrors `g_inet_socket_address_to_native`.
    pub fn to_native(&self, dest: &mut [u8]) -> Result<(), crate::gioerror::IOErrorEnum> {
        match self.address.family() {
            SocketFamily::Ipv4 => {
                if dest.len() < SockaddrIn::SIZE {
                    return Err(crate::gioerror::IOErrorEnum::NoSpace);
                }
                let bytes = self.address.to_bytes();
                if bytes.len() != 4 {
                    return Err(crate::gioerror::IOErrorEnum::Failed);
                }
                let mut addr = [0u8; 4];
                addr.copy_from_slice(&bytes[..4]);
                let sock = SockaddrIn {
                    sin_family: SocketFamily::Ipv4 as u16,
                    sin_port: g_htons(self.port),
                    sin_addr: addr,
                    sin_zero: [0u8; 8],
                };
                let src = unsafe {
                    core::slice::from_raw_parts(
                        &sock as *const SockaddrIn as *const u8,
                        SockaddrIn::SIZE,
                    )
                };
                dest[..SockaddrIn::SIZE].copy_from_slice(src);
                Ok(())
            }
            SocketFamily::Ipv6 => {
                if dest.len() < SockaddrIn6::SIZE {
                    return Err(crate::gioerror::IOErrorEnum::NoSpace);
                }
                let bytes = self.address.to_bytes();
                if bytes.len() != 16 {
                    return Err(crate::gioerror::IOErrorEnum::Failed);
                }
                let mut addr = [0u8; 16];
                addr.copy_from_slice(&bytes[..16]);
                let sock = SockaddrIn6 {
                    sin6_family: SocketFamily::Ipv6 as u16,
                    sin6_port: g_htons(self.port),
                    sin6_flowinfo: self.flowinfo.to_be(),
                    sin6_addr: addr,
                    sin6_scope_id: self.scope_id,
                };
                let src = unsafe {
                    core::slice::from_raw_parts(
                        &sock as *const SockaddrIn6 as *const u8,
                        SockaddrIn6::SIZE,
                    )
                };
                dest[..SockaddrIn6::SIZE].copy_from_slice(src);
                Ok(())
            }
            _ => Err(crate::gioerror::IOErrorEnum::NotSupported),
        }
    }

    /// Deserializes a native `struct sockaddr_in` or `struct sockaddr_in6`
    /// into an `InetSocketAddress`.
    ///
    /// Mirrors the IPv4/IPv6 branches of `g_socket_address_new_from_native`.
    pub fn from_native(native: &[u8]) -> Option<Self> {
        if native.len() < 2 {
            return None;
        }

        let family = u16::from_ne_bytes([native[0], native[1]]);

        match family {
            2 => {
                // AF_INET
                if native.len() < SockaddrIn::SIZE {
                    return None;
                }
                let port = g_ntohs(u16::from_ne_bytes([native[2], native[3]]));
                let addr_bytes = [native[4], native[5], native[6], native[7]];
                let addr = InetAddress::new_from_bytes(&addr_bytes, SocketFamily::Ipv4)?;
                Some(InetSocketAddress::new(addr, port))
            }
            10 => {
                // AF_INET6
                if native.len() < SockaddrIn6::SIZE {
                    return None;
                }
                let port = g_ntohs(u16::from_ne_bytes([native[2], native[3]]));
                let flowinfo = u32::from_be_bytes([native[4], native[5], native[6], native[7]]);
                let addr_bytes: [u8; 16] = native[8..24].try_into().ok()?;
                let scope_id = u32::from_ne_bytes([native[24], native[25], native[26], native[27]]);

                // Check for IPv4-mapped IPv6 address (::ffff:a.b.c.d)
                if addr_bytes[0..10] == [0u8; 10]
                    && addr_bytes[10] == 0xff
                    && addr_bytes[11] == 0xff
                {
                    let v4_bytes = [
                        addr_bytes[12],
                        addr_bytes[13],
                        addr_bytes[14],
                        addr_bytes[15],
                    ];
                    let addr = InetAddress::new_from_bytes(&v4_bytes, SocketFamily::Ipv4)?;
                    Some(InetSocketAddress::new(addr, port))
                } else {
                    let addr = InetAddress::new_from_bytes(&addr_bytes, SocketFamily::Ipv6)?;
                    Some(InetSocketAddress::new_with_ipv6_info(
                        addr, port, flowinfo, scope_id,
                    ))
                }
            }
            _ => None,
        }
    }

    /// Returns the connectable string representation.
    ///
    /// For IPv4: `"addr:port"`. For IPv6 with non-zero scope_id:
    /// `"[addr%scope_id]:port"`. For IPv6: `"[addr]:port"`.
    /// If port is 0, only the address is returned (no `:port` suffix).
    ///
    /// Mirrors `g_inet_socket_address_connectable_to_string`.
    pub fn to_string(&self) -> String {
        let addr_str = self.address.to_string();
        let mut out = String::new();

        // Address.
        out.push_str(&addr_str);

        // Scope ID (IPv6 only).
        if self.address.family() == SocketFamily::Ipv6 && self.scope_id != 0 {
            out.push_str(&format!("%{}", self.scope_id));
        }

        // Port.
        if self.port != 0 {
            if self.address.family() == SocketFamily::Ipv6 {
                // Disambiguate ports from IPv6 addresses using brackets.
                let bracketed = format!("[{}]:{}", out, self.port);
                return bracketed;
            }
            out.push_str(&format!(":{}", self.port));
        }

        out
    }

    /// Compares two `InetSocketAddress` values for equality.
    ///
    /// Two addresses are equal if they have the same `InetAddress`,
    /// port, flowinfo, and scope_id.
    pub fn equal(&self, other: &Self) -> bool {
        self.address == other.address
            && self.port == other.port
            && self.flowinfo == other.flowinfo
            && self.scope_id == other.scope_id
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_ipv4() {
        let addr = InetAddress::new_from_string("192.168.1.1").unwrap();
        let sa = InetSocketAddress::new(addr.clone(), 8080);
        assert_eq!(sa.address(), &addr);
        assert_eq!(sa.port(), 8080);
        assert_eq!(sa.family(), SocketFamily::Ipv4);
        assert_eq!(sa.flowinfo(), 0);
        assert_eq!(sa.scope_id(), 0);
    }

    #[test]
    fn test_new_ipv6() {
        let addr = InetAddress::new_from_string("2001:db8::1").unwrap();
        let sa = InetSocketAddress::new_with_ipv6_info(addr.clone(), 443, 0x12345, 42);
        assert_eq!(sa.address(), &addr);
        assert_eq!(sa.port(), 443);
        assert_eq!(sa.family(), SocketFamily::Ipv6);
        assert_eq!(sa.flowinfo(), 0x12345);
        assert_eq!(sa.scope_id(), 42);
    }

    #[test]
    fn test_new_from_string_ipv4() {
        let sa = InetSocketAddress::new_from_string("10.0.0.1", 80).unwrap();
        assert_eq!(sa.address().to_string(), "10.0.0.1");
        assert_eq!(sa.port(), 80);
    }

    #[test]
    fn test_new_from_string_invalid() {
        assert!(InetSocketAddress::new_from_string("not-an-ip", 80).is_none());
        assert!(InetSocketAddress::new_from_string("192.168.1", 80).is_none());
    }

    #[test]
    fn test_flowinfo_ipv4_returns_zero() {
        let addr = InetAddress::new_from_string("1.2.3.4").unwrap();
        let sa = InetSocketAddress::new_with_ipv6_info(addr, 80, 999, 888);
        assert_eq!(sa.flowinfo(), 0);
        assert_eq!(sa.scope_id(), 0);
    }

    #[test]
    fn test_native_size() {
        let v4 = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
        assert_eq!(v4.native_size(), SockaddrIn::SIZE);

        let v6 = InetSocketAddress::new(InetAddress::new_from_string("::1").unwrap(), 80);
        assert_eq!(v6.native_size(), SockaddrIn6::SIZE);
    }

    #[test]
    fn test_to_native_ipv4() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("192.168.1.1").unwrap(), 8080);
        let mut buf = [0u8; SockaddrIn::SIZE];
        sa.to_native(&mut buf).unwrap();

        // Check family (AF_INET = 2, stored as u16 in native byte order)
        let family = u16::from_ne_bytes([buf[0], buf[1]]);
        assert_eq!(family, 2);

        // Check port (network byte order)
        let port = g_ntohs(u16::from_ne_bytes([buf[2], buf[3]]));
        assert_eq!(port, 8080);

        // Check address bytes
        assert_eq!(&buf[4..8], &[192, 168, 1, 1]);
    }

    #[test]
    fn test_to_native_ipv6() {
        let sa = InetSocketAddress::new_with_ipv6_info(
            InetAddress::new_from_string("2001:db8::1").unwrap(),
            443,
            0,
            0,
        );
        let mut buf = [0u8; SockaddrIn6::SIZE];
        sa.to_native(&mut buf).unwrap();

        // Check family (AF_INET6 = 10)
        let family = u16::from_ne_bytes([buf[0], buf[1]]);
        assert_eq!(family, 10);

        // Check port
        let port = g_ntohs(u16::from_ne_bytes([buf[2], buf[3]]));
        assert_eq!(port, 443);

        // Check address bytes (first bytes of 2001:db8::1)
        assert_eq!(&buf[8..12], &[0x20, 0x01, 0x0d, 0xb8]);
    }

    #[test]
    fn test_to_native_no_space() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
        let mut buf = [0u8; 8]; // Too small
        assert_eq!(
            sa.to_native(&mut buf),
            Err(crate::gioerror::IOErrorEnum::NoSpace)
        );
    }

    #[test]
    fn test_from_native_ipv4() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("192.168.1.1").unwrap(), 8080);
        let mut buf = [0u8; SockaddrIn::SIZE];
        sa.to_native(&mut buf).unwrap();
        let sa2 = InetSocketAddress::from_native(&buf).unwrap();
        assert_eq!(sa2.address().to_string(), "192.168.1.1");
        assert_eq!(sa2.port(), 8080);
    }

    #[test]
    fn test_from_native_ipv6() {
        let sa = InetSocketAddress::new_with_ipv6_info(
            InetAddress::new_from_string("2001:db8::1").unwrap(),
            443,
            0x12345,
            42,
        );
        let mut buf = [0u8; SockaddrIn6::SIZE];
        sa.to_native(&mut buf).unwrap();
        let sa2 = InetSocketAddress::from_native(&buf).unwrap();
        assert_eq!(sa2.address().to_string(), "2001:db8::1");
        assert_eq!(sa2.port(), 443);
        assert_eq!(sa2.flowinfo(), 0x12345);
        assert_eq!(sa2.scope_id(), 42);
    }

    #[test]
    fn test_from_native_ipv4_mapped_ipv6() {
        // ::ffff:192.168.1.1 should decode as IPv4
        let sa = InetSocketAddress::new(
            InetAddress::new_from_string("::ffff:192.168.1.1").unwrap(),
            80,
        );
        let mut buf = [0u8; SockaddrIn6::SIZE];
        sa.to_native(&mut buf).unwrap();
        let sa2 = InetSocketAddress::from_native(&buf).unwrap();
        assert_eq!(sa2.family(), SocketFamily::Ipv4);
        assert_eq!(sa2.address().to_string(), "192.168.1.1");
        assert_eq!(sa2.port(), 80);
    }

    #[test]
    fn test_from_native_too_small() {
        assert!(InetSocketAddress::from_native(&[0u8; 1]).is_none());
    }

    #[test]
    fn test_from_native_unknown_family() {
        let mut buf = [0u8; SockaddrIn::SIZE];
        buf[0] = 99; // Unknown family
        assert!(InetSocketAddress::from_native(&buf).is_none());
    }

    #[test]
    fn test_to_string_ipv4() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("192.168.1.1").unwrap(), 8080);
        assert_eq!(sa.to_string(), "192.168.1.1:8080");
    }

    #[test]
    fn test_to_string_ipv4_port_zero() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("192.168.1.1").unwrap(), 0);
        assert_eq!(sa.to_string(), "192.168.1.1");
    }

    #[test]
    fn test_to_string_ipv6() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("2001:db8::1").unwrap(), 443);
        assert_eq!(sa.to_string(), "[2001:db8::1]:443");
    }

    #[test]
    fn test_to_string_ipv6_scope_id() {
        let sa = InetSocketAddress::new_with_ipv6_info(
            InetAddress::new_from_string("fe80::1").unwrap(),
            80,
            0,
            5,
        );
        assert_eq!(sa.to_string(), "[fe80::1%5]:80");
    }

    #[test]
    fn test_to_string_ipv6_port_zero() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("::1").unwrap(), 0);
        assert_eq!(sa.to_string(), "::1");
    }

    #[test]
    fn test_equal() {
        let a = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
        let b = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
        let c = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 81);
        let d = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.5").unwrap(), 80);
        assert!(a.equal(&b));
        assert!(!a.equal(&c));
        assert!(!a.equal(&d));
    }

    #[test]
    fn test_equal_ipv6_with_flowinfo_scope() {
        let addr = InetAddress::new_from_string("2001:db8::1").unwrap();
        let a = InetSocketAddress::new_with_ipv6_info(addr.clone(), 80, 100, 200);
        let b = InetSocketAddress::new_with_ipv6_info(addr, 80, 100, 200);
        let c = InetSocketAddress::new_with_ipv6_info(
            InetAddress::new_from_string("2001:db8::1").unwrap(),
            80,
            101,
            200,
        );
        assert!(a.equal(&b));
        assert!(!a.equal(&c));
    }

    #[test]
    fn test_clone() {
        let sa = InetSocketAddress::new(InetAddress::new_from_string("1.2.3.4").unwrap(), 80);
        let sa2 = sa.clone();
        assert!(sa.equal(&sa2));
    }
}
