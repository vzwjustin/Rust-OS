//! GIO IP address matching `gio/ginetaddress.h` / `gio/ginetaddress.c`.
//!
//! Upstream `GInetAddress` is a `GObject` subclass. We port it as a
//! plain `pub struct` with the same API, since the GObject subclassing
//! isn't needed for the pure data + parsing/classification logic.
//! Parsing is hand-written (no `inet_pton` / `getaddrinfo` in
//! `no_std`); supports IPv4 dotted-quad and IPv6 text format
//! (including `::` compression and embedded IPv4).
//!
//! Provides:
//! - `SocketFamily` enum (Invalid / Unix / Ipv4 / Ipv6).
//! - `InetAddress` struct (family + raw bytes) with `new_from_string`,
//!   `new_from_bytes`, `new_loopback`, `new_any`, `equal`, `to_string`,
//!   `to_bytes`, `native_size`, `family`, plus the `is_*` classification
//!   methods (any / loopback / link_local / site_local / multicast and
//!   multicast scopes).
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

// ─────────────────────────── SocketFamily ─────────────────────────────────

/// Socket address family (`GSocketFamily`).
///
/// Discriminant values match Linux's `AF_*` constants
/// (`AF_UNIX = 1`, `AF_INET = 2`, `AF_INET6 = 10`) so the values
/// align with what a real OS would use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SocketFamily {
    /// No address family (`G_SOCKET_FAMILY_INVALID`).
    Invalid = 0,
    /// UNIX domain socket (`G_SOCKET_FAMILY_UNIX`, `AF_UNIX`).
    Unix = 1,
    /// IPv4 (`G_SOCKET_FAMILY_IPV4`, `AF_INET`).
    Ipv4 = 2,
    /// IPv6 (`G_SOCKET_FAMILY_IPV6`, `AF_INET6`).
    Ipv6 = 10,
}

// ────────────────────────── InetAddress ───────────────────────────────────

/// Raw address bytes — 4 for IPv4, 16 for IPv6.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InetAddrBytes {
    /// IPv4 — 4 bytes.
    Ipv4([u8; 4]),
    /// IPv6 — 16 bytes.
    Ipv6([u8; 16]),
}

impl InetAddrBytes {
    fn as_slice(&self) -> &[u8] {
        match self {
            InetAddrBytes::Ipv4(b) => b,
            InetAddrBytes::Ipv6(b) => b,
        }
    }

    fn len(&self) -> usize {
        match self {
            InetAddrBytes::Ipv4(_) => 4,
            InetAddrBytes::Ipv6(_) => 16,
        }
    }
}

/// An IP address (`GInetAddress`).
///
/// Plain struct port of the upstream GObject subclass. Holds the
/// address family and the raw bytes. Owned by Rust's ownership model
/// (no manual ref/unref needed).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InetAddress {
    bytes: InetAddrBytes,
}

impl InetAddress {
    /// Create from raw bytes (`g_inet_address_new_from_bytes`).
    ///
    /// `bytes` must be 4 bytes for IPv4 or 16 bytes for IPv6.
    pub fn new_from_bytes(bytes: &[u8], family: SocketFamily) -> Option<Self> {
        match family {
            SocketFamily::Ipv4 => {
                if bytes.len() != 4 {
                    return None;
                }
                let mut arr = [0u8; 4];
                arr.copy_from_slice(bytes);
                Some(Self { bytes: InetAddrBytes::Ipv4(arr) })
            }
            SocketFamily::Ipv6 => {
                if bytes.len() != 16 {
                    return None;
                }
                let mut arr = [0u8; 16];
                arr.copy_from_slice(bytes);
                Some(Self { bytes: InetAddrBytes::Ipv6(arr) })
            }
            _ => None,
        }
    }

    /// Create the loopback address for `family`
    /// (`g_inet_address_new_loopback`).
    ///
    /// IPv4: `127.0.0.1`. IPv6: `::1`.
    pub fn new_loopback(family: SocketFamily) -> Option<Self> {
        match family {
            SocketFamily::Ipv4 => Self::new_from_bytes(&[127, 0, 0, 1], SocketFamily::Ipv4),
            SocketFamily::Ipv6 => Self::new_from_bytes(
                &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
                SocketFamily::Ipv6,
            ),
            _ => None,
        }
    }

    /// Create the "any" address for `family`
    /// (`g_inet_address_new_any`).
    ///
    /// IPv4: `0.0.0.0`. IPv6: `::`.
    pub fn new_any(family: SocketFamily) -> Option<Self> {
        match family {
            SocketFamily::Ipv4 => Self::new_from_bytes(&[0; 4], SocketFamily::Ipv4),
            SocketFamily::Ipv6 => Self::new_from_bytes(&[0; 16], SocketFamily::Ipv6),
            _ => None,
        }
    }

    /// Parse an IP address from a string (`g_inet_address_new_from_string`).
    ///
    /// Accepts IPv4 dotted-quad (`"192.168.1.1"`) and IPv6 text form
    /// (including `::` compression and embedded IPv4
    /// `"::ffff:192.168.1.1"`). Returns `None` if the string isn't a
    /// valid IP address.
    pub fn new_from_string(string: &str) -> Option<Self> {
        // If it contains ':', it's IPv6 (or invalid). Otherwise IPv4.
        if string.contains(':') {
            parse_ipv6(string)
        } else {
            parse_ipv4(string)
        }
    }

    /// Compare two addresses (`g_inet_address_equal`).
    pub fn equal(&self, other: &InetAddress) -> bool {
        self.bytes == other.bytes
    }

    /// Format as a string (`g_inet_address_to_string`).
    pub fn to_string(&self) -> String {
        match &self.bytes {
            InetAddrBytes::Ipv4(b) => format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3]),
            InetAddrBytes::Ipv6(b) => format_ipv6(b),
        }
    }

    /// Raw bytes (`g_inet_address_to_bytes`).
    pub fn to_bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    /// Native size: 4 for IPv4, 16 for IPv6
    /// (`g_inet_address_get_native_size`).
    pub fn native_size(&self) -> usize {
        self.bytes.len()
    }

    /// Address family (`g_inet_address_get_family`).
    pub fn family(&self) -> SocketFamily {
        match self.bytes {
            InetAddrBytes::Ipv4(_) => SocketFamily::Ipv4,
            InetAddrBytes::Ipv6(_) => SocketFamily::Ipv6,
        }
    }

    // ── classification (`g_inet_address_get_is_*`) ───────────────────

    /// Whether this is the "any" address (all zeros).
    pub fn is_any(&self) -> bool {
        self.to_bytes().iter().all(|&b| b == 0)
    }

    /// Whether this is a loopback address.
    ///
    /// IPv4: `127.0.0.0/8`. IPv6: `::1`.
    pub fn is_loopback(&self) -> bool {
        match &self.bytes {
            InetAddrBytes::Ipv4(b) => b[0] == 127,
            InetAddrBytes::Ipv6(b) => {
                // ::1 — first 15 bytes 0, last byte 1.
                b[0..15].iter().all(|&x| x == 0) && b[15] == 1
            }
        }
    }

    /// Whether this is a link-local address.
    ///
    /// IPv4: `169.254.0.0/16`. IPv6: `fe80::/10`.
    pub fn is_link_local(&self) -> bool {
        match &self.bytes {
            InetAddrBytes::Ipv4(b) => b[0] == 169 && b[1] == 254,
            InetAddrBytes::Ipv6(b) => (b[0] & 0xff) == 0xfe && (b[1] & 0xc0) == 0x80,
        }
    }

    /// Whether this is a site-local address.
    ///
    /// IPv4: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`.
    /// IPv6: `fec0::/10` (deprecated by RFC 3879 but still recognized).
    pub fn is_site_local(&self) -> bool {
        match &self.bytes {
            InetAddrBytes::Ipv4(b) => {
                b[0] == 10
                    || (b[0] == 172 && (b[1] & 0xf0) == 16)
                    || (b[0] == 192 && b[1] == 168)
            }
            InetAddrBytes::Ipv6(b) => (b[0] & 0xff) == 0xfe && (b[1] & 0xc0) == 0xc0,
        }
    }

    /// Whether this is a multicast address.
    ///
    /// IPv4: `224.0.0.0/4`. IPv6: `ff00::/8`.
    pub fn is_multicast(&self) -> bool {
        match &self.bytes {
            InetAddrBytes::Ipv4(b) => (b[0] & 0xf0) == 0xe0,
            InetAddrBytes::Ipv6(b) => b[0] == 0xff,
        }
    }

    /// Whether this is a global-scope multicast address.
    pub fn is_mc_global(&self) -> bool {
        match &self.bytes {
            InetAddrBytes::Ipv4(b) => {
                // 224.0.1.0 – 238.255.255.255 (exclude 224.0.0.x which
                // is link-local, and 239.x which is org-local).
                self.is_multicast() && !(b[0] == 224 && b[1] == 0 && b[2] == 0) && b[0] != 239
            }
            InetAddrBytes::Ipv6(b) => {
                self.is_multicast() && (b[1] & 0x0f) == 0x0e
            }
        }
    }

    /// Whether this is a link-local-scope multicast address.
    pub fn is_mc_link_local(&self) -> bool {
        match &self.bytes {
            InetAddrBytes::Ipv4(b) => {
                // 224.0.0.x — link-local multicast.
                self.is_multicast() && b[0] == 224 && b[1] == 0 && b[2] == 0
            }
            InetAddrBytes::Ipv6(b) => self.is_multicast() && (b[1] & 0x0f) == 0x02,
        }
    }

    /// Whether this is a node-local-scope multicast address.
    pub fn is_mc_node_local(&self) -> bool {
        match &self.bytes {
            InetAddrBytes::Ipv4(_) => false, // IPv4 has no node-local multicast scope.
            InetAddrBytes::Ipv6(b) => self.is_multicast() && (b[1] & 0x0f) == 0x01,
        }
    }

    /// Whether this is an organization-local-scope multicast address.
    pub fn is_mc_org_local(&self) -> bool {
        match &self.bytes {
            InetAddrBytes::Ipv4(b) => {
                // 239.192.0.0/16 — org-local multicast.
                self.is_multicast() && b[0] == 239 && b[1] >= 192
            }
            InetAddrBytes::Ipv6(b) => self.is_multicast() && (b[1] & 0x0f) == 0x08,
        }
    }

    /// Whether this is a site-local-scope multicast address.
    pub fn is_mc_site_local(&self) -> bool {
        match &self.bytes {
            InetAddrBytes::Ipv4(b) => {
                // 239.255.0.0/16 — site-local multicast.
                self.is_multicast() && b[0] == 239 && b[1] == 255
            }
            InetAddrBytes::Ipv6(b) => self.is_multicast() && (b[1] & 0x0f) == 0x05,
        }
    }
}

// ──────────────────────────── IPv4 parser ─────────────────────────────────

/// Parse an IPv4 dotted-quad string. Returns `None` on malformed input.
fn parse_ipv4(s: &str) -> Option<InetAddress> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut bytes = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        // Reject empty parts, leading zeros (matching inet_pton's
        // strictness for IPv4 — though we allow "0" itself), and
        // non-digit chars.
        if part.is_empty() {
            return None;
        }
        if part.len() > 1 && part.starts_with('0') {
            // Leading zero not allowed (e.g. "01") — matches inet_pton.
            return None;
        }
        let n: u32 = part.parse().ok()?;
        if n > 255 {
            return None;
        }
        bytes[i] = n as u8;
    }
    Some(InetAddress { bytes: InetAddrBytes::Ipv4(bytes) })
}

// ──────────────────────────── IPv6 parser ─────────────────────────────────

/// Parse an IPv6 text-form string. Supports:
/// - Full form: `2001:db8:0:0:0:0:0:1` (8 hex groups).
/// - Compressed `::`: `2001:db8::1`, `::1`, `::`.
/// - Embedded IPv4: `::ffff:192.168.1.1` (last 2 groups as IPv4).
///
/// Returns `None` on malformed input.
fn parse_ipv6(s: &str) -> Option<InetAddress> {
    // Split on "::" — at most one occurrence.
    let mut halves = s.splitn(2, "::");
    let left_str = halves.next().unwrap_or("");
    let right_str = halves.next().unwrap_or("");

    // If there's no "::", the whole string is `left_str`.
    let has_compress = s.contains("::");

    // Parse the left and right halves into groups of 4 hex digits.
    let left_groups: Vec<&str> = if left_str.is_empty() {
        Vec::new()
    } else {
        left_str.split(':').collect()
    };
    let right_groups: Vec<&str> = if right_str.is_empty() {
        Vec::new()
    } else {
        right_str.split(':').collect()
    };

    // Check for embedded IPv4 in the last group of the right half.
    let mut embedded_ipv4_bytes: Option<[u8; 4]> = None;
    let right_groups_no_v4: Vec<&str> = if !right_groups.is_empty() {
        let last = *right_groups.last().unwrap();
        if last.contains('.') {
            // Parse the embedded IPv4.
            let v4 = parse_ipv4(last)?;
            match &v4.bytes {
                InetAddrBytes::Ipv4(b) => {
                    embedded_ipv4_bytes = Some(*b);
                    right_groups[..right_groups.len() - 1].to_vec()
                }
                _ => return None,
            }
        } else {
            right_groups.clone()
        }
    } else {
        right_groups.clone()
    };

    // Total groups: left + (2 for embedded IPv4) + right + (zeros from ::).
    let explicit_groups = left_groups.len() + right_groups_no_v4.len()
        + if embedded_ipv4_bytes.is_some() { 2 } else { 0 };

    if !has_compress && explicit_groups != 8 {
        // Without ::, we need exactly 8 groups.
        return None;
    }
    if explicit_groups > 8 {
        return None;
    }

    let zeros_to_insert = if has_compress {
        8 - explicit_groups
    } else {
        0
    };

    // Assemble the 16-byte address.
    let mut bytes = [0u8; 16];
    let mut pos = 0usize;

    // Left groups.
    for g in &left_groups {
        let val = parse_hex_group(g)?;
        bytes[pos] = (val >> 8) as u8;
        bytes[pos + 1] = val as u8;
        pos += 2;
    }

    // Zeros from ::.
    for _ in 0..(zeros_to_insert * 2) {
        bytes[pos] = 0;
        pos += 1;
    }

    // Right groups (excluding embedded IPv4).
    for g in &right_groups_no_v4 {
        let val = parse_hex_group(g)?;
        bytes[pos] = (val >> 8) as u8;
        bytes[pos + 1] = val as u8;
        pos += 2;
    }

    // Embedded IPv4 bytes.
    if let Some(v4) = embedded_ipv4_bytes {
        bytes[pos] = v4[0];
        bytes[pos + 1] = v4[1];
        bytes[pos + 2] = v4[2];
        bytes[pos + 3] = v4[3];
        pos += 4;
    }

    if pos != 16 {
        return None;
    }

    Some(InetAddress { bytes: InetAddrBytes::Ipv6(bytes) })
}

/// Parse a single 4-hex-digit IPv6 group into a `u16`.
fn parse_hex_group(s: &str) -> Option<u16> {
    if s.is_empty() || s.len() > 4 {
        return None;
    }
    let mut val: u16 = 0;
    for c in s.chars() {
        val = val.checked_mul(16)?;
        let digit = c.to_digit(16)?;
        val = val.checked_add(digit as u16)?;
    }
    Some(val)
}

// ──────────────────────────── IPv6 formatter ──────────────────────────────

/// Format 16 IPv6 bytes as a string, applying `::` compression per
/// RFC 5952 (compress the longest run of zero groups, and only if
/// the run is at least 2 groups).
fn format_ipv6(b: &[u8; 16]) -> String {
    // View as 8 u16 groups (big-endian).
    let groups: [u16; 8] = core::array::from_fn(|i| {
        ((b[i * 2] as u16) << 8) | (b[i * 2 + 1] as u16)
    });

    // Find the longest run of zero groups (at least 2 long) for ::
    // compression. RFC 5952: ties go to the first run.
    let mut best_start = 0usize;
    let mut best_len = 0usize;
    let mut cur_start = 0usize;
    let mut cur_len = 0usize;
    for (i, &g) in groups.iter().enumerate() {
        if g == 0 {
            if cur_len == 0 {
                cur_start = i;
            }
            cur_len += 1;
            if cur_len > best_len {
                best_len = cur_len;
                best_start = cur_start;
            }
        } else {
            cur_len = 0;
        }
    }

    if best_len < 2 {
        // No compression — just join all groups with ':'.
        let parts: Vec<String> = groups.iter().map(|g| format!("{g:x}")).collect();
        return parts.join(":");
    }

    // Build with :: compression.
    let mut s = String::new();
    // Groups before the zero run.
    let before = best_start;
    let after = best_start + best_len;
    if before > 0 {
        for i in 0..before {
            if i > 0 {
                s.push(':');
            }
            s.push_str(&format!("{:x}", groups[i]));
        }
    }
    s.push_str("::");
    if after < 8 {
        for i in after..8 {
            if i > after {
                s.push(':');
            }
            s.push_str(&format!("{:x}", groups[i]));
        }
    }
    s
}

// ───────────────────────────── tests ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_family_values() {
        assert_eq!(SocketFamily::Invalid as i32, 0);
        assert_eq!(SocketFamily::Unix as i32, 1);
        assert_eq!(SocketFamily::Ipv4 as i32, 2);
        assert_eq!(SocketFamily::Ipv6 as i32, 10);
    }

    #[test]
    fn ipv4_from_string_roundtrip() {
        let addr = InetAddress::new_from_string("192.168.1.1").unwrap();
        assert_eq!(addr.family(), SocketFamily::Ipv4);
        assert_eq!(addr.to_bytes(), &[192, 168, 1, 1]);
        assert_eq!(addr.native_size(), 4);
        assert_eq!(addr.to_string(), "192.168.1.1");
    }

    #[test]
    fn ipv4_from_bytes() {
        let addr = InetAddress::new_from_bytes(&[10, 0, 0, 1], SocketFamily::Ipv4).unwrap();
        assert_eq!(addr.family(), SocketFamily::Ipv4);
        assert_eq!(addr.to_string(), "10.0.0.1");
    }

    #[test]
    fn ipv4_loopback_and_any() {
        let lo = InetAddress::new_loopback(SocketFamily::Ipv4).unwrap();
        assert_eq!(lo.to_string(), "127.0.0.1");
        assert!(lo.is_loopback());
        let any = InetAddress::new_any(SocketFamily::Ipv4).unwrap();
        assert_eq!(any.to_string(), "0.0.0.0");
        assert!(any.is_any());
    }

    #[test]
    fn ipv4_invalid_strings() {
        assert!(InetAddress::new_from_string("192.168.1").is_none());
        assert!(InetAddress::new_from_string("192.168.1.256").is_none());
        assert!(InetAddress::new_from_string("192.168.1.1.1").is_none());
        assert!(InetAddress::new_from_string("not-an-ip").is_none());
        assert!(InetAddress::new_from_string("").is_none());
        assert!(InetAddress::new_from_string("01.02.03.04").is_none()); // leading zeros
    }

    #[test]
    fn ipv4_classification() {
        // Loopback 127.0.0.0/8.
        assert!(InetAddress::new_from_string("127.0.0.1").unwrap().is_loopback());
        assert!(InetAddress::new_from_string("127.255.255.254").unwrap().is_loopback());
        assert!(!InetAddress::new_from_string("128.0.0.1").unwrap().is_loopback());
        // Link-local 169.254.0.0/16.
        assert!(InetAddress::new_from_string("169.254.1.1").unwrap().is_link_local());
        assert!(!InetAddress::new_from_string("169.253.1.1").unwrap().is_link_local());
        // Site-local 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16.
        assert!(InetAddress::new_from_string("10.1.2.3").unwrap().is_site_local());
        assert!(InetAddress::new_from_string("172.16.0.1").unwrap().is_site_local());
        assert!(InetAddress::new_from_string("172.31.255.255").unwrap().is_site_local());
        assert!(!InetAddress::new_from_string("172.15.0.1").unwrap().is_site_local());
        assert!(InetAddress::new_from_string("192.168.1.1").unwrap().is_site_local());
        assert!(!InetAddress::new_from_string("11.0.0.0").unwrap().is_site_local());
        // Multicast 224.0.0.0/4.
        assert!(InetAddress::new_from_string("224.0.0.1").unwrap().is_multicast());
        assert!(InetAddress::new_from_string("239.255.255.255").unwrap().is_multicast());
        assert!(!InetAddress::new_from_string("223.255.255.255").unwrap().is_multicast());
        // Multicast scopes.
        assert!(InetAddress::new_from_string("224.0.0.1").unwrap().is_mc_link_local());
        assert!(InetAddress::new_from_string("239.255.0.1").unwrap().is_mc_site_local());
        assert!(InetAddress::new_from_string("239.192.0.1").unwrap().is_mc_org_local());
    }

    #[test]
    fn ipv6_full_form() {
        let addr = InetAddress::new_from_string("2001:0db8:0000:0000:0000:0000:0000:0001").unwrap();
        assert_eq!(addr.family(), SocketFamily::Ipv6);
        assert_eq!(addr.native_size(), 16);
        // Compression should produce "2001:db8::1".
        assert_eq!(addr.to_string(), "2001:db8::1");
    }

    #[test]
    fn ipv6_compressed() {
        let addr = InetAddress::new_from_string("2001:db8::1").unwrap();
        assert_eq!(addr.family(), SocketFamily::Ipv6);
        assert_eq!(addr.to_string(), "2001:db8::1");
    }

    #[test]
    fn ipv6_loopback_and_any() {
        let lo = InetAddress::new_from_string("::1").unwrap();
        assert_eq!(lo.family(), SocketFamily::Ipv6);
        assert!(lo.is_loopback());
        assert_eq!(lo.to_string(), "::1");
        let any = InetAddress::new_from_string("::").unwrap();
        assert!(any.is_any());
        assert_eq!(any.to_string(), "::");
    }

    #[test]
    fn ipv6_new_loopback_and_any() {
        let lo = InetAddress::new_loopback(SocketFamily::Ipv6).unwrap();
        assert_eq!(lo.to_bytes(), &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert!(lo.is_loopback());
        let any = InetAddress::new_any(SocketFamily::Ipv6).unwrap();
        assert!(any.is_any());
        assert_eq!(any.to_bytes(), &[0; 16]);
    }

    #[test]
    fn ipv6_embedded_ipv4() {
        // ::ffff:192.168.1.1 — IPv4-mapped IPv6.
        let addr = InetAddress::new_from_string("::ffff:192.168.1.1").unwrap();
        assert_eq!(addr.family(), SocketFamily::Ipv6);
        let expected = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 192, 168, 1, 1];
        assert_eq!(addr.to_bytes(), &expected[..]);
    }

    #[test]
    fn ipv6_classification() {
        // Link-local fe80::/10.
        assert!(InetAddress::new_from_string("fe80::1").unwrap().is_link_local());
        assert!(InetAddress::new_from_string("febf::1").unwrap().is_link_local());
        assert!(!InetAddress::new_from_string("fec0::1").unwrap().is_link_local());
        // Site-local fec0::/10 (deprecated).
        assert!(InetAddress::new_from_string("fec0::1").unwrap().is_site_local());
        assert!(!InetAddress::new_from_string("fe80::1").unwrap().is_site_local());
        // Multicast ff00::/8.
        assert!(InetAddress::new_from_string("ff02::1").unwrap().is_multicast());
        assert!(!InetAddress::new_from_string("fe02::1").unwrap().is_multicast());
        // Multicast scopes.
        assert!(InetAddress::new_from_string("ff02::1").unwrap().is_mc_link_local());
        assert!(InetAddress::new_from_string("ff01::1").unwrap().is_mc_node_local());
        assert!(InetAddress::new_from_string("ff05::1").unwrap().is_mc_site_local());
        assert!(InetAddress::new_from_string("ff08::1").unwrap().is_mc_org_local());
        assert!(InetAddress::new_from_string("ff0e::1").unwrap().is_mc_global());
    }

    #[test]
    fn ipv6_invalid_strings() {
        assert!(InetAddress::new_from_string("2001:db8::1::1").is_none()); // double ::
        assert!(InetAddress::new_from_string("2001:db8:gggg::1").is_none()); // non-hex
        assert!(InetAddress::new_from_string("::12345").is_none()); // group too long
        assert!(InetAddress::new_from_string("not-an-ip").is_none());
        assert!(InetAddress::new_from_string("").is_none());
    }

    #[test]
    fn equal_addresses() {
        let a = InetAddress::new_from_string("192.168.1.1").unwrap();
        let b = InetAddress::new_from_string("192.168.1.1").unwrap();
        let c = InetAddress::new_from_string("192.168.1.2").unwrap();
        assert!(a.equal(&b));
        assert!(!a.equal(&c));
        // IPv4 vs IPv6 with same first 4 bytes shouldn't be equal.
        let v6 = InetAddress::new_from_string("::192.168.1.1").unwrap();
        assert!(!a.equal(&v6));
    }

    #[test]
    fn format_ipv6_compression_picks_longest_run() {
        // 2001:db8:0:0:0:0:0:1 → longest zero run is 5 groups →
        // "2001:db8::1".
        let addr = InetAddress::new_from_string("2001:db8:0:0:0:0:0:1").unwrap();
        assert_eq!(addr.to_string(), "2001:db8::1");
        // ::1 → "0:0:0:0:0:0:0:1" compressed to "::1".
        let addr = InetAddress::new_from_string("0:0:0:0:0:0:0:1").unwrap();
        assert_eq!(addr.to_string(), "::1");
    }

    #[test]
    fn invalid_family_returns_none() {
        assert!(InetAddress::new_from_bytes(&[1, 2, 3, 4], SocketFamily::Invalid).is_none());
        assert!(InetAddress::new_from_bytes(&[1, 2, 3, 4], SocketFamily::Unix).is_none());
        assert!(InetAddress::new_loopback(SocketFamily::Invalid).is_none());
        assert!(InetAddress::new_any(SocketFamily::Invalid).is_none());
    }

    #[test]
    fn wrong_byte_count_returns_none() {
        assert!(InetAddress::new_from_bytes(&[1, 2, 3], SocketFamily::Ipv4).is_none());
        assert!(InetAddress::new_from_bytes(&[1, 2, 3, 4, 5], SocketFamily::Ipv4).is_none());
        assert!(InetAddress::new_from_bytes(&[0; 15], SocketFamily::Ipv6).is_none());
        assert!(InetAddress::new_from_bytes(&[0; 17], SocketFamily::Ipv6).is_none());
    }

    #[test]
    fn clone_preserves_fields() {
        let addr = InetAddress::new_from_string("192.168.1.1").unwrap();
        let cloned = addr.clone();
        assert_eq!(addr, cloned);
        assert!(addr.equal(&cloned));
    }
}
