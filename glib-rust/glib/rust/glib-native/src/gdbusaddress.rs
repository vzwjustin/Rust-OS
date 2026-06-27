//! GDBusAddress matching `gio/gdbusaddress.h` / `gio/gdbusaddress.c`.
//!
//! D-Bus address parsing, validation, escaping, and bus-address lookup.
//! On bare metal the in-process `"loopback:"` transport is always supported;
//! session/system addresses come from a [`DBusAddressPlatform`] hook.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gdbusproxy::DBusBusType;
use crate::gioerror::{io_error_quark, IOErrorEnum};
use crate::uri::unescape_string;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::RwLock;

/// A parsed single D-Bus address entry (`unix:path=/tmp/bus`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DBusAddress {
    pub transport: String,
    pub params: BTreeMap<String, String>,
}

impl DBusAddress {
    /// Parses one address entry like `unix:path=/tmp/dbus`.
    pub fn parse_entry(address: &str) -> Result<Self, Error> {
        parse_entry(address)
    }

    /// Parses a full address string (semicolon-separated entries).
    pub fn parse_all(address: &str) -> Result<Vec<Self>, Error> {
        parse_all(address)
    }

    /// Gets a parameter value by key.
    pub fn get_param(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(|s| s.as_str())
    }

    /// Serializes this entry back to a D-Bus address fragment.
    pub fn to_entry_string(&self) -> String {
        let mut out = self.transport.clone();
        out.push(':');
        for (i, (k, v)) in self.params.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(k);
            out.push('=');
            out.push_str(&dbus_address_escape_value(v));
        }
        out
    }

    pub fn is_unix(&self) -> bool {
        self.transport == "unix"
    }

    pub fn is_tcp(&self) -> bool {
        self.transport == "tcp"
    }

    pub fn is_nonce_tcp(&self) -> bool {
        self.transport == "nonce-tcp"
    }

    pub fn is_loopback(&self) -> bool {
        self.transport == "loopback"
    }

    pub fn is_autolaunch(&self) -> bool {
        self.transport == "autolaunch"
    }

    pub fn is_launchd(&self) -> bool {
        self.transport == "launchd"
    }
}

/// Escapes a value for use in a D-Bus address (`g_dbus_address_escape_value`).
pub fn dbus_address_escape_value(string: &str) -> String {
    let mut out = String::with_capacity(string.len());
    for ch in string.chars() {
        match ch {
            '%' => out.push_str("%25"),
            ',' => out.push_str("%2C"),
            '=' => out.push_str("%3D"),
            ';' => out.push_str("%3B"),
            ':' => out.push_str("%3A"),
            '~' => out.push_str("%7E"),
            '\\' => {
                out.push('\\');
                out.push(ch);
            }
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/' => {
                out.push(c)
            }
            c if c.is_ascii() => out.push_str(&format!("%{:02X}", c as u8)),
            c => {
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                for b in encoded.bytes() {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    out
}

/// Checks if `string` is a valid D-Bus address (`g_dbus_is_address`).
pub fn is_address(string: &str) -> bool {
    parse_all(string).is_ok()
}

/// Checks if the address is supported by this library (`g_dbus_is_supported_address`).
pub fn is_supported_address(string: &str) -> Result<(), Error> {
    for entry in parse_all(string)? {
        if !is_supported_entry(&entry)? {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                format!("unsupported transport {:?}", entry.transport),
            ));
        }
    }
    Ok(())
}

/// Looks up the address for a well-known bus (`g_dbus_address_get_for_bus_sync`).
pub fn dbus_address_get_for_bus_sync(bus_type: DBusBusType) -> Result<String, Error> {
    match bus_type {
        DBusBusType::None => Ok(String::from("loopback:")),
        DBusBusType::Session => DBUS_ADDRESS_PLATFORM
            .read()
            .get_session_bus_address()
            .ok_or_else(|| {
                Error::new(
                    io_error_quark(),
                    IOErrorEnum::NotSupported.to_code(),
                    "session bus address unavailable",
                )
            }),
        DBusBusType::System => DBUS_ADDRESS_PLATFORM
            .read()
            .get_system_bus_address()
            .ok_or_else(|| {
                Error::new(
                    io_error_quark(),
                    IOErrorEnum::NotSupported.to_code(),
                    "system bus address unavailable",
                )
            }),
    }
}

/// Platform hook for session/system bus addresses.
pub trait DBusAddressPlatform: Send + Sync {
    fn get_session_bus_address(&self) -> Option<String>;
    fn get_system_bus_address(&self) -> Option<String>;
}

/// Default bare-metal platform — only `loopback:` via [`dbus_address_get_for_bus_sync`].
pub struct NoDBusAddressPlatform;

impl DBusAddressPlatform for NoDBusAddressPlatform {
    fn get_session_bus_address(&self) -> Option<String> {
        None
    }

    fn get_system_bus_address(&self) -> Option<String> {
        None
    }
}

static DBUS_ADDRESS_PLATFORM: RwLock<&'static dyn DBusAddressPlatform> =
    RwLock::new(&NoDBusAddressPlatform);

/// Installs the platform implementation for bus address lookup.
pub fn register_dbus_address_platform(platform: &'static dyn DBusAddressPlatform) {
    *DBUS_ADDRESS_PLATFORM.write() = platform;
}

/// Back-compat: session bus address from the platform hook.
pub fn get_session_bus_address() -> Option<String> {
    DBUS_ADDRESS_PLATFORM.read().get_session_bus_address()
}

/// Back-compat: system bus address from the platform hook.
pub fn get_system_bus_address() -> Option<String> {
    DBUS_ADDRESS_PLATFORM.read().get_system_bus_address()
}

fn parse_all(address: &str) -> Result<Vec<DBusAddress>, Error> {
    if address.is_empty() {
        return Err(invalid_arg("empty address"));
    }
    address
        .split(';')
        .filter(|s| !s.is_empty())
        .map(parse_entry)
        .collect()
}

fn parse_entry(address_entry: &str) -> Result<DBusAddress, Error> {
    let colon = address_entry
        .find(':')
        .ok_or_else(|| invalid_arg("address entry does not contain a colon"))?;
    if colon == 0 {
        return Err(invalid_arg("transport name must not be empty"));
    }
    let transport = address_entry[..colon].to_string();
    let rest = &address_entry[colon + 1..];
    let mut params = BTreeMap::new();
    if !rest.is_empty() {
        for (n, kv) in rest.split(',').enumerate() {
            if kv.is_empty() {
                continue;
            }
            let eq = kv
                .find('=')
                .ok_or_else(|| invalid_arg(&format!("pair {n} missing '='")))?;
            if eq == 0 {
                return Err(invalid_arg(&format!("pair {n} has empty key")));
            }
            let key = unescape_segment(&kv[..eq])?;
            let value = unescape_segment(&kv[eq + 1..])?;
            params.insert(key, value);
        }
    }
    Ok(DBusAddress { transport, params })
}

fn unescape_segment(s: &str) -> Result<String, Error> {
    unescape_string(s).map_err(|_| invalid_arg("invalid escape sequence"))
}

fn invalid_arg(msg: &str) -> Error {
    Error::new(
        io_error_quark(),
        IOErrorEnum::InvalidArgument.to_code(),
        msg,
    )
}

fn is_supported_entry(entry: &DBusAddress) -> Result<bool, Error> {
    match entry.transport.as_str() {
        "loopback" => Ok(true),
        "unix" => validate_unix(entry),
        "tcp" => validate_tcp(entry),
        "nonce-tcp" => validate_nonce_tcp(entry),
        "autolaunch" => Ok(entry.params.is_empty()),
        _ => Ok(false),
    }
}

fn validate_unix(entry: &DBusAddress) -> Result<bool, Error> {
    let mut path_keys = 0usize;
    for (key, _) in &entry.params {
        match key.as_str() {
            "path" | "dir" | "tmpdir" | "abstract" => path_keys += 1,
            "guid" => {}
            other => {
                return Err(invalid_arg(&format!("unsupported unix key {other}")));
            }
        }
    }
    if path_keys != 1 {
        return Err(invalid_arg(
            "unix address needs exactly one of path, dir, tmpdir, abstract",
        ));
    }
    Ok(true)
}

fn validate_tcp(entry: &DBusAddress) -> Result<bool, Error> {
    validate_tcp_like(entry, false)
}

fn validate_nonce_tcp(entry: &DBusAddress) -> Result<bool, Error> {
    validate_tcp_like(entry, true)
}

fn validate_tcp_like(entry: &DBusAddress, require_nonce: bool) -> Result<bool, Error> {
    let mut has_nonce = false;
    for (key, value) in &entry.params {
        match key.as_str() {
            "host" => {}
            "port" => {
                let port: u32 = value.parse().map_err(|_| invalid_arg("malformed port"))?;
                if port >= 65536 {
                    return Err(invalid_arg("port out of range"));
                }
            }
            "family" => {
                if value != "ipv4" && value != "ipv6" {
                    return Err(invalid_arg("family must be ipv4 or ipv6"));
                }
            }
            "noncefile" => has_nonce = true,
            "guid" => {}
            other => return Err(invalid_arg(&format!("unsupported tcp key {other}"))),
        }
    }
    if require_nonce && !has_nonce {
        return Err(invalid_arg("nonce-tcp requires noncefile"));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPlatform;
    impl DBusAddressPlatform for MockPlatform {
        fn get_session_bus_address(&self) -> Option<String> {
            Some(String::from("unix:path=/tmp/session-bus"))
        }
        fn get_system_bus_address(&self) -> Option<String> {
            Some(String::from("unix:path=/var/run/dbus/system_bus_socket"))
        }
    }

    #[test]
    fn test_parse_unix() {
        let addr = DBusAddress::parse_entry("unix:path=/tmp/dbus-socket").unwrap();
        assert!(addr.is_unix());
        assert_eq!(addr.get_param("path"), Some("/tmp/dbus-socket"));
    }

    #[test]
    fn test_parse_tcp() {
        let addr = DBusAddress::parse_entry("tcp:host=localhost,port=12345").unwrap();
        assert!(addr.is_tcp());
        assert_eq!(addr.get_param("host"), Some("localhost"));
        assert_eq!(addr.get_param("port"), Some("12345"));
    }

    #[test]
    fn test_parse_multiple() {
        let addrs = DBusAddress::parse_all("unix:path=/a;tcp:host=127.0.0.1,port=1").unwrap();
        assert_eq!(addrs.len(), 2);
    }

    #[test]
    fn test_escape_value() {
        assert_eq!(
            dbus_address_escape_value("/run/bus-for-:0"),
            "/run/bus-for-%3A0"
        );
        assert_eq!(dbus_address_escape_value("tilde~"), "tilde%7E");
    }

    #[test]
    fn test_is_supported_loopback() {
        assert!(is_supported_address("loopback:").is_ok());
        assert!(is_supported_address("unix:path=/tmp/x").is_ok());
        assert!(is_supported_address("tcp:host=127.0.0.1,port=42").is_ok());
        assert!(is_supported_address("bogus:foo=bar").is_err());
    }

    #[test]
    fn test_get_for_bus_none() {
        let addr = dbus_address_get_for_bus_sync(DBusBusType::None).unwrap();
        assert_eq!(addr, "loopback:");
    }

    #[test]
    fn test_platform_hook() {
        register_dbus_address_platform(&MockPlatform);
        let session = dbus_address_get_for_bus_sync(DBusBusType::Session).unwrap();
        assert!(session.contains("/tmp/session-bus"));
        register_dbus_address_platform(&NoDBusAddressPlatform);
    }
}
