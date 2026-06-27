//! `giowin32-afunix` matching `gio/giowin32-afunix.h`.
//!
//! Fallback definitions for Windows AF_UNIX socket support.
//!
//! Fully `no_std` compatible.

/// Maximum path length for Unix domain sockets on Windows
/// (mirrors `UNIX_PATH_MAX`).
pub const UNIX_PATH_MAX: usize = 108;

/// Windows AF_UNIX socket address (mirrors `SOCKADDR_UN`).
#[derive(Debug, Clone)]
pub struct SockaddrUn {
    pub sun_family: u16,
    pub sun_path: [u8; UNIX_PATH_MAX],
}

impl SockaddrUn {
    /// Creates a new empty AF_UNIX address.
    pub fn new() -> Self {
        Self {
            sun_family: 1, // AF_UNIX
            sun_path: [0u8; UNIX_PATH_MAX],
        }
    }

    /// Creates an AF_UNIX address from a path string.
    pub fn from_path(path: &str) -> Self {
        let mut addr = Self::new();
        let bytes = path.as_bytes();
        let len = bytes.len().min(UNIX_PATH_MAX - 1);
        addr.sun_path[..len].copy_from_slice(&bytes[..len]);
        addr
    }

    /// Returns the path as a string (up to the first null byte).
    pub fn path(&self) -> &str {
        let end = self
            .sun_path
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(UNIX_PATH_MAX);
        core::str::from_utf8(&self.sun_path[..end]).unwrap_or("")
    }
}

impl Default for SockaddrUn {
    fn default() -> Self {
        Self::new()
    }
}

/// IO control code for getting the peer PID of an AF_UNIX socket
/// (mirrors `SIO_AF_UNIX_GETPEERPID`).
pub const SIO_AF_UNIX_GETPEERPID: u32 = 0x98000064; // _WSAIOR(IOC_VENDOR, 256)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unix_path_max() {
        assert_eq!(UNIX_PATH_MAX, 108);
    }

    #[test]
    fn test_sockaddr_un_new() {
        let addr = SockaddrUn::new();
        assert_eq!(addr.sun_family, 1);
        assert_eq!(addr.path(), "");
    }

    #[test]
    fn test_from_path() {
        let addr = SockaddrUn::from_path("/tmp/socket");
        assert_eq!(addr.path(), "/tmp/socket");
    }

    #[test]
    fn test_long_path_truncated() {
        let long_path = "a".repeat(200);
        let addr = SockaddrUn::from_path(&long_path);
        let path = addr.path();
        assert!(path.len() <= UNIX_PATH_MAX);
    }
}
