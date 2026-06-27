//! GIO IP address mask matching `gio/ginetaddressmask.h` /
//! `gio/ginetaddressmask.c`.
//!
//! Upstream `GInetAddressMask` is a `GObject` subclass implementing
//! `GInitable`. We port it as a plain `pub struct` with `Result`-
//! returning constructors (replacing the upstream `GError**` pattern).
//! Depends only on `InetAddress` and `SocketFamily` (both ported in
//! `ginetaddress`).
//!
//! Provides:
//! - `InetAddressMaskError` enum (InvalidArgument / LengthTooLong /
//!   BitsBeyondPrefix) matching the upstream `G_IO_ERROR_INVALID_ARGUMENT`
//!   cases.
//! - `InetAddressMask` struct (base address + prefix length) with
//!   `new` (validates bits beyond `length` are 0), `new_from_string`
//!   (parses `"addr/length"` or `"addr"`), `to_string`, `family`,
//!   `address`, `length`, `matches`, `equal`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use crate::ginetaddress::{InetAddress, SocketFamily};
use alloc::string::String;
use alloc::format;

// ─────────────────────── InetAddressMaskError ─────────────────────────────

/// Errors returned by `InetAddressMask::new` /
/// `InetAddressMask::new_from_string`. Mirrors the
/// `G_IO_ERROR_INVALID_ARGUMENT` cases in
/// `g_inet_address_mask_initable_init` and `_new_from_string`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InetAddressMaskError {
    /// No address specified (upstream: "No address specified").
    NoAddress,
    /// Length is too long for the address family
    /// (upstream: "Length %u is too long for address").
    LengthTooLong,
    /// Address has bits set beyond the prefix length
    /// (upstream: "Address has bits set beyond prefix length").
    BitsBeyondPrefix,
    /// Could not parse the string as an IP address mask
    /// (upstream: "Could not parse ... as IP address mask").
    ParseFailed,
}

// ──────────────────────── InetAddressMask ─────────────────────────────────

/// An IP address mask (`GInetAddressMask`).
///
/// Represents all addresses whose first `length` bits match the base
/// `address`. Plain struct port of the upstream GObject subclass.
/// Holds a base `InetAddress` and a prefix length (0–32 for IPv4,
/// 0–128 for IPv6).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InetAddressMask {
    addr: InetAddress,
    length: u32,
}

impl InetAddressMask {
    /// Create a new mask from `addr` and `length`
    /// (`g_inet_address_mask_new`).
    ///
    /// Validates that `length <= addr.native_size() * 8` and that all
    /// bits in `addr` beyond position `length` are 0 (matching the
    /// upstream `g_inet_address_mask_initable_init` checks). Returns
    /// an error otherwise.
    pub fn new(addr: InetAddress, length: u32) -> Result<Self, InetAddressMaskError> {
        let addrlen = addr.native_size(); // 4 or 16 bytes
        let max_length = (addrlen * 8) as u32;
        if length > max_length {
            return Err(InetAddressMaskError::LengthTooLong);
        }
        // Verify all bits after `length` are 0.
        let bytes = addr.to_bytes();
        let nbytes = (length / 8) as usize;
        let nbits = (length % 8) as u32;
        // Check the partial byte: bits below position nbits must be 0.
        if nbits != 0 && nbytes < bytes.len() {
            let mask = 0xffu8 >> nbits;
            if bytes[nbytes] & mask != 0 {
                return Err(InetAddressMaskError::BitsBeyondPrefix);
            }
        }
        // Check the remaining full bytes after the partial byte.
        let start = if nbits != 0 { nbytes + 1 } else { nbytes };
        for &b in &bytes[start..] {
            if b != 0 {
                return Err(InetAddressMaskError::BitsBeyondPrefix);
            }
        }
        Ok(Self { addr, length })
    }

    /// Parse a mask from a string (`g_inet_address_mask_new_from_string`).
    ///
    /// Accepts `"addr/length"` or just `"addr"` (in which case the
    /// length is the full address size in bits). Returns an error if
    /// the address can't be parsed or the length is invalid.
    pub fn new_from_string(s: &str) -> Result<Self, InetAddressMaskError> {
        if let Some(slash_idx) = s.find('/') {
            let addr_str = &s[..slash_idx];
            let len_str = &s[slash_idx + 1..];
            if len_str.is_empty() {
                return Err(InetAddressMaskError::ParseFailed);
            }
            // Parse length — must be all digits, no trailing garbage.
            let length: u32 = len_str
                .parse()
                .map_err(|_| InetAddressMaskError::ParseFailed)?;
            let addr = InetAddress::new_from_string(addr_str)
                .ok_or(InetAddressMaskError::ParseFailed)?;
            Self::new(addr, length)
        } else {
            // No '/' — full-length mask.
            let addr = InetAddress::new_from_string(s)
                .ok_or(InetAddressMaskError::ParseFailed)?;
            let length = (addr.native_size() * 8) as u32;
            Self::new(addr, length)
        }
    }

    /// Format as a string (`g_inet_address_mask_to_string`).
    ///
    /// Returns `"addr/length"`, or just `"addr"` if `length` is the
    /// full address size.
    pub fn to_string(&self) -> String {
        let addr_str = self.addr.to_string();
        let max_length = (self.addr.native_size() * 8) as u32;
        if self.length == max_length {
            addr_str
        } else {
            format!("{addr_str}/{}", self.length)
        }
    }

    /// Address family (`g_inet_address_mask_get_family`).
    pub fn family(&self) -> SocketFamily {
        self.addr.family()
    }

    /// Base address (`g_inet_address_mask_get_address`).
    pub fn address(&self) -> &InetAddress {
        &self.addr
    }

    /// Prefix length in bits (`g_inet_address_mask_get_length`).
    pub fn length(&self) -> u32 {
        self.length
    }

    /// Test whether `address` falls within the range described by this
    /// mask (`g_inet_address_mask_matches`).
    ///
    /// Returns `false` if the families differ. Returns `true` if
    /// `length == 0` (matches everything). Otherwise compares the
    /// first `length` bits of the two addresses.
    pub fn matches(&self, address: &InetAddress) -> bool {
        if self.addr.family() != address.family() {
            return false;
        }
        if self.length == 0 {
            return true;
        }
        let mask_bytes = self.addr.to_bytes();
        let addr_bytes = address.to_bytes();
        let nbytes = (self.length / 8) as usize;
        // Compare full bytes.
        if nbytes != 0 && mask_bytes[..nbytes] != addr_bytes[..nbytes] {
            return false;
        }
        let nbits = self.length % 8;
        if nbits == 0 {
            return true;
        }
        // Compare the partial byte: mask the high nbits bits.
        let bitmask: u8 = 0xff << (8 - nbits);
        mask_bytes[nbytes] == (addr_bytes[nbytes] & bitmask)
    }

    /// Test whether two masks are equal
    /// (`g_inet_address_mask_equal`).
    ///
    /// Two masks are equal if they have the same length and the same
    /// base address.
    pub fn equal(&self, other: &InetAddressMask) -> bool {
        self.length == other.length && self.addr.equal(&other.addr)
    }
}

// ───────────────────────────── tests ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ipv4_full_length() {
        let addr = InetAddress::new_from_string("192.168.1.0").unwrap();
        let mask = InetAddressMask::new(addr.clone(), 24).unwrap();
        assert_eq!(mask.family(), SocketFamily::Ipv4);
        assert_eq!(mask.length(), 24);
        assert_eq!(mask.address().to_string(), "192.168.1.0");
    }

    #[test]
    fn new_ipv4_full_32_bits() {
        let addr = InetAddress::new_from_string("192.168.1.1").unwrap();
        let mask = InetAddressMask::new(addr, 32).unwrap();
        assert_eq!(mask.length(), 32);
    }

    #[test]
    fn new_ipv4_zero_length() {
        let addr = InetAddress::new_from_string("0.0.0.0").unwrap();
        let mask = InetAddressMask::new(addr, 0).unwrap();
        assert_eq!(mask.length(), 0);
    }

    #[test]
    fn new_ipv4_length_too_long() {
        let addr = InetAddress::new_from_string("192.168.1.0").unwrap();
        // IPv4 max is 32 bits.
        let result = InetAddressMask::new(addr, 33);
        assert_eq!(result.unwrap_err(), InetAddressMaskError::LengthTooLong);
    }

    #[test]
    fn new_ipv4_bits_beyond_prefix() {
        // 192.168.1.1 with length 24 — the .1 in the last byte is
        // beyond the prefix, should fail.
        let addr = InetAddress::new_from_string("192.168.1.1").unwrap();
        let result = InetAddressMask::new(addr, 24);
        assert_eq!(result.unwrap_err(), InetAddressMaskError::BitsBeyondPrefix);
    }

    #[test]
    fn new_ipv4_partial_byte_beyond_prefix() {
        // 192.168.1.128 with length 25 — the low 7 bits of .128 are
        // beyond the prefix (25 = 24 + 1 bit), so the remaining 7 bits
        // must be 0. .128 = 0b10000000, low 7 bits = 0, so this is OK.
        let addr = InetAddress::new_from_string("192.168.1.128").unwrap();
        let mask = InetAddressMask::new(addr, 25).unwrap();
        assert_eq!(mask.length(), 25);
        // 192.168.1.129 with length 25 — .129 = 0b10000001, low 7 bits
        // != 0, should fail.
        let addr2 = InetAddress::new_from_string("192.168.1.129").unwrap();
        let result = InetAddressMask::new(addr2, 25);
        assert_eq!(result.unwrap_err(), InetAddressMaskError::BitsBeyondPrefix);
    }

    #[test]
    fn new_ipv6_full_length() {
        let addr = InetAddress::new_from_string("2001:db8::").unwrap();
        let mask = InetAddressMask::new(addr, 32).unwrap();
        assert_eq!(mask.family(), SocketFamily::Ipv6);
        assert_eq!(mask.length(), 32);
    }

    #[test]
    fn new_ipv6_length_too_long() {
        let addr = InetAddress::new_from_string("2001:db8::").unwrap();
        // IPv6 max is 128 bits.
        let result = InetAddressMask::new(addr, 129);
        assert_eq!(result.unwrap_err(), InetAddressMaskError::LengthTooLong);
    }

    #[test]
    fn from_string_ipv4_with_length() {
        let mask = InetAddressMask::new_from_string("192.168.1.0/24").unwrap();
        assert_eq!(mask.family(), SocketFamily::Ipv4);
        assert_eq!(mask.length(), 24);
        assert_eq!(mask.address().to_string(), "192.168.1.0");
    }

    #[test]
    fn from_string_ipv4_without_length() {
        // No length → full length (32 for IPv4).
        let mask = InetAddressMask::new_from_string("192.168.1.1").unwrap();
        assert_eq!(mask.length(), 32);
        assert_eq!(mask.to_string(), "192.168.1.1");
    }

    #[test]
    fn from_string_ipv6_with_length() {
        let mask = InetAddressMask::new_from_string("2001:db8::/32").unwrap();
        assert_eq!(mask.family(), SocketFamily::Ipv6);
        assert_eq!(mask.length(), 32);
        assert_eq!(mask.to_string(), "2001:db8::/32");
    }

    #[test]
    fn from_string_ipv6_without_length() {
        let mask = InetAddressMask::new_from_string("2001:db8::1").unwrap();
        assert_eq!(mask.length(), 128);
        assert_eq!(mask.to_string(), "2001:db8::1");
    }

    #[test]
    fn from_string_invalid() {
        assert_eq!(
            InetAddressMask::new_from_string("not-an-ip").unwrap_err(),
            InetAddressMaskError::ParseFailed
        );
        assert_eq!(
            InetAddressMask::new_from_string("192.168.1.0/").unwrap_err(),
            InetAddressMaskError::ParseFailed
        );
        assert_eq!(
            InetAddressMask::new_from_string("192.168.1.0/notanumber").unwrap_err(),
            InetAddressMaskError::ParseFailed
        );
        // Bits beyond prefix — the constructor catches this.
        assert_eq!(
            InetAddressMask::new_from_string("192.168.1.1/24").unwrap_err(),
            InetAddressMaskError::BitsBeyondPrefix
        );
        // Length too long.
        assert_eq!(
            InetAddressMask::new_from_string("192.168.1.0/33").unwrap_err(),
            InetAddressMaskError::LengthTooLong
        );
    }

    #[test]
    fn to_string_omits_full_length() {
        let mask = InetAddressMask::new_from_string("192.168.1.0/32").unwrap();
        assert_eq!(mask.to_string(), "192.168.1.0");
        let mask = InetAddressMask::new_from_string("192.168.1.0/24").unwrap();
        assert_eq!(mask.to_string(), "192.168.1.0/24");
    }

    #[test]
    fn matches_ipv4() {
        let mask = InetAddressMask::new_from_string("192.168.1.0/24").unwrap();
        assert!(mask.matches(&InetAddress::new_from_string("192.168.1.1").unwrap()));
        assert!(mask.matches(&InetAddress::new_from_string("192.168.1.255").unwrap()));
        assert!(!mask.matches(&InetAddress::new_from_string("192.168.2.1").unwrap()));
        assert!(!mask.matches(&InetAddress::new_from_string("10.0.0.1").unwrap()));
    }

    #[test]
    fn matches_ipv4_partial_byte() {
        // 192.168.1.128/25 — matches 192.168.1.128–255.
        let mask = InetAddressMask::new_from_string("192.168.1.128/25").unwrap();
        assert!(mask.matches(&InetAddress::new_from_string("192.168.1.128").unwrap()));
        assert!(mask.matches(&InetAddress::new_from_string("192.168.1.255").unwrap()));
        assert!(!mask.matches(&InetAddress::new_from_string("192.168.1.127").unwrap()));
        assert!(!mask.matches(&InetAddress::new_from_string("192.168.0.255").unwrap()));
    }

    #[test]
    fn matches_zero_length_matches_everything() {
        let mask = InetAddressMask::new(InetAddress::new_from_string("0.0.0.0").unwrap(), 0).unwrap();
        assert!(mask.matches(&InetAddress::new_from_string("192.168.1.1").unwrap()));
        assert!(mask.matches(&InetAddress::new_from_string("10.0.0.1").unwrap()));
    }

    #[test]
    fn matches_full_length_matches_only_self() {
        let mask = InetAddressMask::new_from_string("192.168.1.1/32").unwrap();
        assert!(mask.matches(&InetAddress::new_from_string("192.168.1.1").unwrap()));
        assert!(!mask.matches(&InetAddress::new_from_string("192.168.1.2").unwrap()));
    }

    #[test]
    fn matches_different_family_returns_false() {
        let mask = InetAddressMask::new_from_string("192.168.1.0/24").unwrap();
        assert!(!mask.matches(&InetAddress::new_from_string("::1").unwrap()));
    }

    #[test]
    fn matches_ipv6() {
        let mask = InetAddressMask::new_from_string("2001:db8::/32").unwrap();
        assert!(mask.matches(&InetAddress::new_from_string("2001:db8::1").unwrap()));
        assert!(mask.matches(&InetAddress::new_from_string("2001:db8:abcd::1").unwrap()));
        assert!(!mask.matches(&InetAddress::new_from_string("2001:db9::1").unwrap()));
    }

    #[test]
    fn equal_masks() {
        let a = InetAddressMask::new_from_string("192.168.1.0/24").unwrap();
        let b = InetAddressMask::new_from_string("192.168.1.0/24").unwrap();
        let c = InetAddressMask::new_from_string("192.168.1.0/25").unwrap();
        let d = InetAddressMask::new_from_string("192.168.2.0/24").unwrap();
        assert!(a.equal(&b));
        assert!(!a.equal(&c)); // different length
        assert!(!a.equal(&d)); // different address
    }

    #[test]
    fn clone_preserves_fields() {
        let mask = InetAddressMask::new_from_string("192.168.1.0/24").unwrap();
        let cloned = mask.clone();
        assert!(mask.equal(&cloned));
        assert_eq!(mask.to_string(), cloned.to_string());
    }
}
