//! GDBusUtils matching `gio/gdbusutils.h`.
//!
//! D-Bus utility functions for validating names, generating GUIDs,
//! and escaping object paths.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

static GUID_COUNTER: Mutex<u64> = Mutex::new(0);

/// Generates a D-Bus GUID.
///
/// Mirrors `g_dbus_generate_guid`.
pub fn generate_guid() -> String {
    let mut counter = GUID_COUNTER.lock();
    *counter += 1;
    let ts = *counter;
    alloc::format!("{:016x}-rustos", ts)
}

/// Checks if a string is a valid D-Bus GUID.
///
/// Mirrors `g_dbus_is_guid`.
pub fn is_guid(string: &str) -> bool {
    if string.len() < 16 {
        return false;
    }
    string.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Checks if a string is a valid D-Bus name (e.g. `org.freedesktop.DBus`).
///
/// Mirrors `g_dbus_is_name`.
pub fn is_name(string: &str) -> bool {
    if string.is_empty() || string.len() > 255 {
        return false;
    }
    if string.starts_with(':') {
        return is_unique_name(string);
    }
    is_interface_name(string)
}

/// Checks if a string is a valid unique D-Bus name (e.g. `:1.42`).
///
/// Mirrors `g_dbus_is_unique_name`.
pub fn is_unique_name(string: &str) -> bool {
    if !string.starts_with(':') {
        return false;
    }
    let rest = &string[1..];
    if rest.is_empty() {
        return false;
    }
    rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
}

/// Checks if a string is a valid D-Bus member name (e.g. `MethodName`).
///
/// Mirrors `g_dbus_is_member_name`.
pub fn is_member_name(string: &str) -> bool {
    if string.is_empty() {
        return false;
    }
    if !string
        .chars()
        .next()
        .map(|c| c.is_ascii_alphabetic() || c == '_')
        .unwrap_or(false)
    {
        return false;
    }
    string
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Checks if a string is a valid D-Bus interface name (e.g. `org.freedesktop.DBus`).
///
/// Mirrors `g_dbus_is_interface_name`.
pub fn is_interface_name(string: &str) -> bool {
    if string.is_empty() || string.len() > 255 {
        return false;
    }
    let parts: Vec<&str> = string.split('.').collect();
    if parts.len() < 2 {
        return false;
    }
    parts.iter().all(|p| is_member_name(p))
}

/// Escapes a string for use in a D-Bus object path.
///
/// Mirrors `g_dbus_escape_object_path`.
pub fn escape_object_path(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
        } else {
            result.push_str(&alloc::format!("{:02x}", c as u32));
        }
    }
    result
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_guid() {
        let guid = generate_guid();
        assert!(!guid.is_empty());
    }

    #[test]
    fn test_generate_guid_unique() {
        let g1 = generate_guid();
        let g2 = generate_guid();
        assert_ne!(g1, g2);
    }

    #[test]
    fn test_is_name_valid() {
        assert!(is_name("org.freedesktop.DBus"));
        assert!(is_name(":1.42"));
    }

    #[test]
    fn test_is_name_invalid() {
        assert!(!is_name(""));
        assert!(!is_name("singleword"));
        assert!(!is_name("a."));
    }

    #[test]
    fn test_is_unique_name() {
        assert!(is_unique_name(":1.42"));
        assert!(is_unique_name(":1.2.3"));
        assert!(!is_unique_name("1.42"));
        assert!(!is_unique_name(":"));
    }

    #[test]
    fn test_is_member_name() {
        assert!(is_member_name("MethodName"));
        assert!(is_member_name("_private"));
        assert!(is_member_name("method123"));
        assert!(!is_member_name(""));
        assert!(!is_member_name("123method"));
        assert!(!is_member_name("method-name"));
    }

    #[test]
    fn test_is_interface_name() {
        assert!(is_interface_name("org.freedesktop.DBus"));
        assert!(!is_interface_name("singleword"));
        assert!(!is_interface_name("org."));
        assert!(!is_interface_name(""));
    }

    #[test]
    fn test_escape_object_path() {
        assert_eq!(escape_object_path("hello"), "hello");
        assert_eq!(escape_object_path("a/b"), "a2fb");
        assert_eq!(escape_object_path("test 123"), "test20123");
    }
}
