//! GIO UNIX domain socket address matching `gio/gunixsocketaddress.h` /
//! `gio/gunixsocketaddress.c`.
//!
//! Upstream `GUnixSocketAddress` is a `GObject` subclass that also
//! implements `GSocketConnectable`. We port it as a plain `pub struct`
//! with the same API, since the GObject subclassing / interface system
//! is deferred (Phase 9).
//!
//! Provides:
//! - `UnixSocketAddressType` enum (Invalid / Anonymous / Path / Abstract /
//!   AbstractPadded).
//! - `UnixSocketAddress` struct (path + address_type).
//! - `new(path)`, `new_with_type(path, path_len, type)`.
//! - `path()`, `path_len()`, `address_type()`, `is_abstract()`.
//! - `family()`, `native_size()`, `to_native()`, `from_native()`.
//! - `to_string()` — connectable string with non-printable char escaping.
//! - `abstract_names_supported()` — runtime check.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::ginetaddress::SocketFamily;
use crate::gioerror::IOErrorEnum;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

/// Maximum length of `sun_path` in `struct sockaddr_un` (Linux: 108).
pub const UNIX_PATH_MAX: usize = 108;

/// `struct sockaddr_un` (110 bytes on Linux: 2-byte family + 108-byte path).
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SockaddrUn {
    /// Address family (`AF_UNIX = 1`).
    pub sun_family: u16,
    /// Path name (108 bytes on Linux).
    pub sun_path: [u8; UNIX_PATH_MAX],
}

impl SockaddrUn {
    /// Size of `struct sockaddr_un`.
    pub const SIZE: usize = 2 + UNIX_PATH_MAX;

    /// Offset of `sun_path` within `struct sockaddr_un`.
    pub const PATH_OFFSET: usize = 2;
}

/// The type of name used by a `UnixSocketAddress` (`GUnixSocketAddressType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum UnixSocketAddressType {
    /// Invalid.
    Invalid = 0,
    /// Anonymous — not bound to any name.
    Anonymous = 1,
    /// A filesystem path.
    Path = 2,
    /// An abstract name (not zero-padded).
    Abstract = 3,
    /// An abstract name, zero-padded to full length.
    AbstractPadded = 4,
}

impl Default for UnixSocketAddressType {
    fn default() -> Self {
        UnixSocketAddressType::Path
    }
}

/// A UNIX domain socket address (`GUnixSocketAddress`).
///
/// Corresponds to `struct sockaddr_un` in the BSD sockets API.
/// Plain struct port of the upstream GObject subclass.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UnixSocketAddress {
    /// The path bytes (not including the initial NUL for abstract names).
    path: Vec<u8>,
    /// The address type.
    address_type: UnixSocketAddressType,
}

impl UnixSocketAddress {
    /// Creates a new `UnixSocketAddress` for a filesystem path.
    ///
    /// Mirrors `g_unix_socket_address_new`.
    pub fn new(path: &str) -> Self {
        UnixSocketAddress {
            path: path.as_bytes().to_vec(),
            address_type: UnixSocketAddressType::Path,
        }
    }

    /// Creates a new `UnixSocketAddress` of the given type with name `path`.
    ///
    /// If `type` is `Anonymous`, `path` is ignored.
    /// If `path_len` is `None`, the path is assumed to be NUL-terminated.
    /// Mirrors `g_unix_socket_address_new_with_type`.
    pub fn new_with_type(
        path: &[u8],
        path_len: Option<usize>,
        addr_type: UnixSocketAddressType,
    ) -> Self {
        if addr_type == UnixSocketAddressType::Anonymous {
            return UnixSocketAddress {
                path: Vec::new(),
                address_type: addr_type,
            };
        }

        let len = match path_len {
            Some(l) => l.min(path.len()).min(UNIX_PATH_MAX - 1),
            None => {
                // NUL-terminated: find the first NUL or use full length.
                let nul_pos = path.iter().position(|&b| b == 0);
                match nul_pos {
                    Some(p) => p.min(UNIX_PATH_MAX - 1),
                    None => path.len().min(UNIX_PATH_MAX - 1),
                }
            }
        };

        UnixSocketAddress {
            path: path[..len].to_vec(),
            address_type: addr_type,
        }
    }

    /// Gets the path (zero-terminated for filesystem paths, name bytes for
    /// abstract sockets).
    ///
    /// Mirrors `g_unix_socket_address_get_path`.
    pub fn path(&self) -> &[u8] {
        &self.path
    }

    /// Gets the length of the path.
    ///
    /// Mirrors `g_unix_socket_address_get_path_len`.
    pub fn path_len(&self) -> usize {
        self.path.len()
    }

    /// Gets the address type.
    ///
    /// Mirrors `g_unix_socket_address_get_address_type`.
    pub fn address_type(&self) -> UnixSocketAddressType {
        self.address_type
    }

    /// Tests if the address is abstract.
    ///
    /// Mirrors `g_unix_socket_address_get_is_abstract` (deprecated).
    pub fn is_abstract(&self) -> bool {
        self.address_type == UnixSocketAddressType::Abstract
            || self.address_type == UnixSocketAddressType::AbstractPadded
    }

    /// Checks if abstract UNIX domain socket names are supported.
    ///
    /// Returns `true` on Linux, `false` otherwise.
    /// Mirrors `g_unix_socket_address_abstract_names_supported`.
    pub fn abstract_names_supported() -> bool {
        cfg!(any(target_os = "linux", target_os = "none"))
    }

    /// Gets the socket family (always `SocketFamily::Unix`).
    pub fn family(&self) -> SocketFamily {
        SocketFamily::Unix
    }

    /// Gets the size of the native `struct sockaddr_un` representation.
    ///
    /// - `Anonymous`: `PATH_OFFSET` (2 bytes, no path).
    /// - `Abstract`: `PATH_OFFSET + path_len + 1` (NUL indicator + name).
    /// - Other: full `SockaddrUn::SIZE`.
    ///
    /// Mirrors `g_unix_socket_address_get_native_size`.
    pub fn native_size(&self) -> usize {
        match self.address_type {
            UnixSocketAddressType::Anonymous => SockaddrUn::PATH_OFFSET,
            UnixSocketAddressType::Abstract => SockaddrUn::PATH_OFFSET + self.path.len() + 1,
            _ => SockaddrUn::SIZE,
        }
    }

    /// Serializes this socket address to a native `struct sockaddr_un` in
    /// `dest`.
    ///
    /// Returns `Ok(())` on success, or an `IOErrorEnum` on failure.
    /// Mirrors `g_unix_socket_address_to_native`.
    pub fn to_native(&self, dest: &mut [u8]) -> Result<(), IOErrorEnum> {
        let socklen = self.native_size();
        if dest.len() < socklen {
            return Err(IOErrorEnum::NoSpace);
        }

        // Zero-fill the buffer.
        dest[..socklen].fill(0);

        // Set family (AF_UNIX = 1).
        dest[0] = 1; // AF_UNIX low byte
        dest[1] = 0; // AF_UNIX high byte

        match self.address_type {
            UnixSocketAddressType::Invalid | UnixSocketAddressType::Anonymous => {
                // No path to write.
            }
            UnixSocketAddressType::Path => {
                // Copy path as a C string (NUL-terminated).
                let path_start = SockaddrUn::PATH_OFFSET;
                let copy_len = self.path.len().min(UNIX_PATH_MAX - 1);
                dest[path_start..path_start + copy_len].copy_from_slice(&self.path[..copy_len]);
            }
            UnixSocketAddressType::Abstract | UnixSocketAddressType::AbstractPadded => {
                if !Self::abstract_names_supported() {
                    return Err(IOErrorEnum::NotSupported);
                }
                // First byte is NUL (abstract indicator), then the name.
                let path_start = SockaddrUn::PATH_OFFSET;
                dest[path_start] = 0;
                let copy_len = self.path.len().min(UNIX_PATH_MAX - 1);
                dest[path_start + 1..path_start + 1 + copy_len]
                    .copy_from_slice(&self.path[..copy_len]);
            }
        }

        Ok(())
    }

    /// Deserializes a native `struct sockaddr_un` into a
    /// `UnixSocketAddress`.
    ///
    /// Mirrors the AF_UNIX branch of `g_socket_address_new_from_native`.
    pub fn from_native(native: &[u8]) -> Option<Self> {
        if native.len() < SockaddrUn::PATH_OFFSET {
            return None;
        }

        let family = u16::from_ne_bytes([native[0], native[1]]);
        if family != 1 {
            // AF_UNIX
            return None;
        }

        let path_data = &native[SockaddrUn::PATH_OFFSET..];

        if path_data.is_empty() || path_data.iter().all(|&b| b == 0) {
            // No path — anonymous.
            return Some(UnixSocketAddress {
                path: Vec::new(),
                address_type: UnixSocketAddressType::Anonymous,
            });
        }

        if path_data[0] == 0 {
            // Abstract socket: name starts after the leading NUL.
            // Find the extent of the name.
            if native.len() >= SockaddrUn::SIZE {
                // Full sockaddr_un — could be abstract padded.
                // Find the last non-zero byte in sun_path (after the leading NUL).
                let name = &path_data[1..];
                let last_non_zero = name.iter().rposition(|&b| b != 0);
                let name_len = match last_non_zero {
                    Some(p) => p + 1,
                    None => 0,
                };
                if name_len == 0 {
                    return Some(UnixSocketAddress {
                        path: Vec::new(),
                        address_type: UnixSocketAddressType::Anonymous,
                    });
                }
                // If there are trailing zeros, it's AbstractPadded; otherwise Abstract.
                let has_trailing_zeros =
                    name_len < name.len() && name[name_len..].iter().any(|&b| b == 0);
                let addr_type = if has_trailing_zeros {
                    UnixSocketAddressType::AbstractPadded
                } else {
                    UnixSocketAddressType::Abstract
                };
                return Some(UnixSocketAddress {
                    path: name[..name_len].to_vec(),
                    address_type: addr_type,
                });
            } else {
                // Shorter than full sockaddr_un — abstract (not padded).
                let name = &path_data[1..];
                let last_non_zero = name.iter().rposition(|&b| b != 0)?;
                return Some(UnixSocketAddress {
                    path: name[..last_non_zero + 1].to_vec(),
                    address_type: UnixSocketAddressType::Abstract,
                });
            }
        }

        // Filesystem path: NUL-terminated string.
        let nul_pos = path_data.iter().position(|&b| b == 0);
        let path_len = match nul_pos {
            Some(p) => p,
            None => path_data.len(),
        };
        Some(UnixSocketAddress {
            path: path_data[..path_len].to_vec(),
            address_type: UnixSocketAddressType::Path,
        })
    }

    /// Returns the connectable string representation.
    ///
    /// For anonymous sockets, returns `"anonymous"`.
    /// For other types, returns the path with non-printable characters
    /// escaped as `\xNN`.
    ///
    /// Mirrors `g_unix_socket_address_connectable_to_string`.
    pub fn to_string(&self) -> String {
        if self.address_type == UnixSocketAddressType::Anonymous {
            return String::from("anonymous");
        }

        let mut out = String::new();
        for &c in &self.path {
            if (0x20..=0x7e).contains(&c) {
                out.push(c as char);
            } else {
                out.push_str(&format!("\\x{:02x}", c));
            }
        }
        out
    }

    /// Compares two `UnixSocketAddress` values for equality.
    pub fn equal(&self, other: &Self) -> bool {
        self.path == other.path && self.address_type == other.address_type
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_path() {
        let sa = UnixSocketAddress::new("/tmp/socket");
        assert_eq!(sa.path(), b"/tmp/socket");
        assert_eq!(sa.path_len(), 11);
        assert_eq!(sa.address_type(), UnixSocketAddressType::Path);
        assert!(!sa.is_abstract());
        assert_eq!(sa.family(), SocketFamily::Unix);
    }

    #[test]
    fn test_new_with_type_anonymous() {
        let sa =
            UnixSocketAddress::new_with_type(b"ignored", None, UnixSocketAddressType::Anonymous);
        assert_eq!(sa.path_len(), 0);
        assert_eq!(sa.address_type(), UnixSocketAddressType::Anonymous);
        assert!(!sa.is_abstract());
    }

    #[test]
    fn test_new_with_type_abstract() {
        let sa = UnixSocketAddress::new_with_type(b"test", None, UnixSocketAddressType::Abstract);
        assert_eq!(sa.path(), b"test");
        assert_eq!(sa.address_type(), UnixSocketAddressType::Abstract);
        assert!(sa.is_abstract());
    }

    #[test]
    fn test_new_with_type_abstract_padded() {
        let sa =
            UnixSocketAddress::new_with_type(b"test", None, UnixSocketAddressType::AbstractPadded);
        assert_eq!(sa.path(), b"test");
        assert_eq!(sa.address_type(), UnixSocketAddressType::AbstractPadded);
        assert!(sa.is_abstract());
    }

    #[test]
    fn test_new_with_type_path_len() {
        let sa =
            UnixSocketAddress::new_with_type(b"hello world", Some(5), UnixSocketAddressType::Path);
        assert_eq!(sa.path(), b"hello");
        assert_eq!(sa.path_len(), 5);
    }

    #[test]
    fn test_new_with_type_nul_terminated() {
        let sa =
            UnixSocketAddress::new_with_type(b"hello\0world", None, UnixSocketAddressType::Path);
        assert_eq!(sa.path(), b"hello");
        assert_eq!(sa.path_len(), 5);
    }

    #[test]
    fn test_native_size_path() {
        let sa = UnixSocketAddress::new("/tmp/socket");
        assert_eq!(sa.native_size(), SockaddrUn::SIZE);
    }

    #[test]
    fn test_native_size_anonymous() {
        let sa = UnixSocketAddress::new_with_type(b"", None, UnixSocketAddressType::Anonymous);
        assert_eq!(sa.native_size(), SockaddrUn::PATH_OFFSET);
    }

    #[test]
    fn test_native_size_abstract() {
        let sa = UnixSocketAddress::new_with_type(b"test", None, UnixSocketAddressType::Abstract);
        // PATH_OFFSET + 1 (NUL indicator) + 4 (name)
        assert_eq!(sa.native_size(), SockaddrUn::PATH_OFFSET + 1 + 4);
    }

    #[test]
    fn test_native_size_abstract_padded() {
        let sa =
            UnixSocketAddress::new_with_type(b"test", None, UnixSocketAddressType::AbstractPadded);
        assert_eq!(sa.native_size(), SockaddrUn::SIZE);
    }

    #[test]
    fn test_to_native_path() {
        let sa = UnixSocketAddress::new("/tmp/socket");
        let mut buf = vec![0u8; SockaddrUn::SIZE];
        sa.to_native(&mut buf).unwrap();

        // Check family
        let family = u16::from_ne_bytes([buf[0], buf[1]]);
        assert_eq!(family, 1); // AF_UNIX

        // Check path
        let path = &buf[SockaddrUn::PATH_OFFSET..];
        assert_eq!(&path[..11], b"/tmp/socket");
        assert_eq!(path[11], 0); // NUL terminated
    }

    #[test]
    fn test_to_native_anonymous() {
        let sa = UnixSocketAddress::new_with_type(b"", None, UnixSocketAddressType::Anonymous);
        let mut buf = vec![0u8; SockaddrUn::SIZE];
        sa.to_native(&mut buf).unwrap();

        let family = u16::from_ne_bytes([buf[0], buf[1]]);
        assert_eq!(family, 1);
        // Path should be all zeros (only PATH_OFFSET bytes written)
        assert_eq!(&buf[2..SockaddrUn::PATH_OFFSET], &[0u8; 0]);
    }

    #[test]
    fn test_to_native_abstract() {
        if !UnixSocketAddress::abstract_names_supported() {
            return;
        }
        let sa = UnixSocketAddress::new_with_type(b"test", None, UnixSocketAddressType::Abstract);
        let socklen = sa.native_size();
        let mut buf = vec![0u8; socklen];
        sa.to_native(&mut buf).unwrap();

        let family = u16::from_ne_bytes([buf[0], buf[1]]);
        assert_eq!(family, 1);

        // First byte of path is NUL (abstract indicator)
        assert_eq!(buf[SockaddrUn::PATH_OFFSET], 0);
        // Then the name
        assert_eq!(&buf[SockaddrUn::PATH_OFFSET + 1..], b"test");
    }

    #[test]
    fn test_to_native_no_space() {
        let sa = UnixSocketAddress::new("/tmp/socket");
        let mut buf = [0u8; 4];
        assert_eq!(sa.to_native(&mut buf), Err(IOErrorEnum::NoSpace));
    }

    #[test]
    fn test_from_native_path() {
        let sa = UnixSocketAddress::new("/var/run/socket");
        let mut buf = vec![0u8; SockaddrUn::SIZE];
        sa.to_native(&mut buf).unwrap();
        let sa2 = UnixSocketAddress::from_native(&buf).unwrap();
        assert_eq!(sa2.address_type(), UnixSocketAddressType::Path);
        assert_eq!(sa2.path(), b"/var/run/socket");
    }

    #[test]
    fn test_from_native_anonymous() {
        let sa = UnixSocketAddress::new_with_type(b"", None, UnixSocketAddressType::Anonymous);
        let socklen = sa.native_size();
        let mut buf = vec![0u8; socklen];
        sa.to_native(&mut buf).unwrap();
        let sa2 = UnixSocketAddress::from_native(&buf).unwrap();
        assert_eq!(sa2.address_type(), UnixSocketAddressType::Anonymous);
        assert_eq!(sa2.path_len(), 0);
    }

    #[test]
    fn test_from_native_abstract() {
        // Construct native buffer manually (abstract to_native requires Linux).
        let mut buf = vec![0u8; SockaddrUn::PATH_OFFSET + 1 + 4];
        buf[0] = 1; // AF_UNIX
        buf[1] = 0;
        buf[SockaddrUn::PATH_OFFSET] = 0; // abstract indicator
        buf[SockaddrUn::PATH_OFFSET + 1..SockaddrUn::PATH_OFFSET + 5].copy_from_slice(b"test");
        let sa = UnixSocketAddress::from_native(&buf).unwrap();
        assert_eq!(sa.address_type(), UnixSocketAddressType::Abstract);
        assert_eq!(sa.path(), b"test");
    }

    #[test]
    fn test_from_native_abstract_padded() {
        // Construct native buffer manually (abstract to_native requires Linux).
        let mut buf = vec![0u8; SockaddrUn::SIZE];
        buf[0] = 1; // AF_UNIX
        buf[1] = 0;
        buf[SockaddrUn::PATH_OFFSET] = 0; // abstract indicator
        buf[SockaddrUn::PATH_OFFSET + 1..SockaddrUn::PATH_OFFSET + 5].copy_from_slice(b"test");
        // Rest is zero-padded (already zeroed by vec![0u8; ...])
        let sa = UnixSocketAddress::from_native(&buf).unwrap();
        assert_eq!(sa.address_type(), UnixSocketAddressType::AbstractPadded);
        assert_eq!(sa.path(), b"test");
    }

    #[test]
    fn test_from_native_roundtrip_path() {
        let original = UnixSocketAddress::new("/tmp/mysocket");
        let mut buf = vec![0u8; SockaddrUn::SIZE];
        original.to_native(&mut buf).unwrap();
        let rt = UnixSocketAddress::from_native(&buf).unwrap();
        assert!(original.equal(&rt));
    }

    #[test]
    fn test_from_native_too_small() {
        assert!(UnixSocketAddress::from_native(&[0u8; 1]).is_none());
    }

    #[test]
    fn test_from_native_wrong_family() {
        let mut buf = vec![0u8; SockaddrUn::SIZE];
        buf[0] = 2; // AF_INET, not AF_UNIX
        assert!(UnixSocketAddress::from_native(&buf).is_none());
    }

    #[test]
    fn test_to_string_path() {
        let sa = UnixSocketAddress::new("/tmp/socket");
        assert_eq!(sa.to_string(), "/tmp/socket");
    }

    #[test]
    fn test_to_string_anonymous() {
        let sa = UnixSocketAddress::new_with_type(b"", None, UnixSocketAddressType::Anonymous);
        assert_eq!(sa.to_string(), "anonymous");
    }

    #[test]
    fn test_to_string_non_printable() {
        let sa =
            UnixSocketAddress::new_with_type(b"ab\x01cd", Some(5), UnixSocketAddressType::Path);
        assert_eq!(sa.to_string(), "ab\\x01cd");
    }

    #[test]
    fn test_equal() {
        let a = UnixSocketAddress::new("/tmp/socket");
        let b = UnixSocketAddress::new("/tmp/socket");
        let c = UnixSocketAddress::new("/tmp/other");
        assert!(a.equal(&b));
        assert!(!a.equal(&c));
    }

    #[test]
    fn test_equal_different_type() {
        let a = UnixSocketAddress::new("test");
        let b = UnixSocketAddress::new_with_type(b"test", None, UnixSocketAddressType::Abstract);
        assert!(!a.equal(&b));
    }

    #[test]
    fn test_clone() {
        let sa = UnixSocketAddress::new("/tmp/socket");
        let sa2 = sa.clone();
        assert!(sa.equal(&sa2));
    }

    #[test]
    fn test_default_address_type() {
        let default: UnixSocketAddressType = Default::default();
        assert_eq!(default, UnixSocketAddressType::Path);
    }
}
