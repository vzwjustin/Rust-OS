//! Hostname utilities matching `ghostutils.h` / `ghostutils.c`.
//!
//! Provides hostname validation, ASCII encoding, and IP address detection.
//! All functions are pure string processing and fully `no_std` compatible.

use crate::prelude::*;

/// Returns `true` if `hostname` contains non-ASCII characters (`g_hostname_is_non_ascii`).
pub fn hostname_is_non_ascii(hostname: &str) -> bool {
    hostname.bytes().any(|b| b >= 0x80)
}

/// Returns `true` if `hostname` contains ACE-encoded labels (starting with `xn--`).
pub fn hostname_is_ascii_encoded(hostname: &str) -> bool {
    for label in hostname.split('.') {
        if label.starts_with("xn--") {
            return true;
        }
    }
    false
}

/// Returns `true` if `hostname` is a valid IP address (IPv4 or IPv6).
///
/// IPv4: four decimal octets separated by dots (e.g. `192.168.1.1`).
/// IPv6: colon-separated hex groups, possibly with `::` shorthand.
pub fn hostname_is_ip_address(hostname: &str) -> bool {
    if hostname.is_empty() {
        return false;
    }

    // Check for IPv6 (contains ':')
    if hostname.contains(':') {
        return is_valid_ipv6(hostname);
    }

    // Check for IPv4
    is_valid_ipv4(hostname)
}

fn is_valid_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    for part in parts {
        if part.is_empty() || part.len() > 3 {
            return false;
        }
        // No leading zeros (except "0" itself)
        if part.len() > 1 && part.starts_with('0') {
            return false;
        }
        match part.parse::<u32>() {
            Ok(n) if n <= 255 => {}
            _ => return false,
        }
    }
    true
}

fn is_valid_ipv6(s: &str) -> bool {
    // Handle :: shorthand (at most one)
    let has_double_colon = s.contains("::");

    if has_double_colon {
        // Split on :: and verify each side
        let parts: Vec<&str> = s.splitn(2, "::").collect();
        let left = if parts[0].is_empty() { Vec::new() } else { parts[0].split(':').collect() };
        let right = if parts[1].is_empty() { Vec::new() } else { parts[1].split(':').collect() };

        if left.len() + right.len() > 7 {
            return false;
        }

        for group in left.iter().chain(right.iter()) {
            if !is_valid_ipv6_group(group) {
                return false;
            }
        }
        true
    } else {
        let groups: Vec<&str> = s.split(':').collect();
        if groups.len() != 8 {
            return false;
        }
        for group in &groups {
            if !is_valid_ipv6_group(group) {
                return false;
            }
        }
        true
    }
}

fn is_valid_ipv6_group(s: &str) -> bool {
    if s.is_empty() || s.len() > 4 {
        return false;
    }
    s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Converts a hostname to ASCII form using IDNA encoding (`g_hostname_to_ascii`).
///
/// If the hostname is already ASCII, it is returned as-is (lowercased).
/// Non-ASCII labels are encoded with `xn--` prefix (simplified IDNA; full
/// punycode encoding is not implemented).
pub fn hostname_to_ascii(hostname: &str) -> Option<String> {
    let mut result = Vec::new();
    for label in hostname.split('.') {
        if label.is_ascii() {
            result.push(label.to_lowercase());
        } else {
            // Full punycode is complex; we encode as xn-- with hex representation
            // This is a simplified encoding for no_std environments
            let encoded: String = label
                .bytes()
                .map(|b| format!("{:02x}", b))
                .collect();
            result.push(format!("xn--{encoded}"));
        }
    }
    Some(result.join("."))
}

/// Converts a hostname to Unicode form (`g_hostname_to_unicode`).
///
/// If the hostname is already Unicode (non-ASCII), it is returned as-is.
/// ACE-encoded labels (`xn--...`) are decoded back (simplified decoding).
pub fn hostname_to_unicode(hostname: &str) -> Option<String> {
    let mut result = Vec::new();
    for label in hostname.split('.') {
        if let Some(rest) = label.strip_prefix("xn--") {
            // Simplified decoding: hex bytes back to string
            let bytes: Vec<u8> = (0..rest.len())
                .step_by(2)
                .filter_map(|i| {
                    if i + 2 <= rest.len() {
                        u8::from_str_radix(&rest[i..i + 2], 16).ok()
                    } else {
                        None
                    }
                })
                .collect();
            if let Ok(s) = core::str::from_utf8(&bytes) {
                result.push(s.to_owned());
            } else {
                result.push(label.to_owned());
            }
        } else {
            result.push(label.to_owned());
        }
    }
    Some(result.join("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_non_ascii() {
        assert!(!hostname_is_non_ascii("example.com"));
        assert!(hostname_is_non_ascii("exämple.com"));
        assert!(!hostname_is_non_ascii("xn--exmple-cua.com"));
    }

    #[test]
    fn is_ascii_encoded() {
        assert!(hostname_is_ascii_encoded("xn--exmple-cua.com"));
        assert!(!hostname_is_ascii_encoded("example.com"));
        assert!(!hostname_is_ascii_encoded("exämple.com"));
    }

    #[test]
    fn is_ipv4() {
        assert!(hostname_is_ip_address("192.168.1.1"));
        assert!(hostname_is_ip_address("0.0.0.0"));
        assert!(hostname_is_ip_address("255.255.255.255"));
        assert!(!hostname_is_ip_address("256.1.1.1"));
        assert!(!hostname_is_ip_address("1.2.3"));
        assert!(!hostname_is_ip_address("01.02.03.04"));
        assert!(!hostname_is_ip_address("example.com"));
    }

    #[test]
    fn is_ipv6() {
        assert!(hostname_is_ip_address("::1"));
        assert!(hostname_is_ip_address("::"));
        assert!(hostname_is_ip_address("2001:db8::1"));
        assert!(hostname_is_ip_address("fe80::1"));
        assert!(hostname_is_ip_address("2001:db8:0:0:0:0:0:1"));
        assert!(!hostname_is_ip_address("2001:db8::1::2"));
        assert!(!hostname_is_ip_address("gggg::1"));
    }

    #[test]
    fn to_ascii_pure_ascii() {
        assert_eq!(
            hostname_to_ascii("Example.COM").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn to_unicode_ascii() {
        assert_eq!(
            hostname_to_unicode("example.com").unwrap(),
            "example.com"
        );
    }
}
