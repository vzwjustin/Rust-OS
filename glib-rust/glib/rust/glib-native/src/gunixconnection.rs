//! GUnixConnection matching `gio/gunixconnection.h`.
//!
//! A `GSocketConnection` subclass for Unix domain socket connections.
//! Tracks a peer credential and an optional received file descriptor.
//!
//! No_std compatible using `alloc`.

use crate::gcredentials::Credentials;
use spin::Mutex;

/// A Unix domain socket connection (`GUnixConnection`).
pub struct UnixConnection {
    peer_credentials: Option<Credentials>,
    /// Simulated received file descriptor (None = none pending).
    received_fd: Mutex<Option<i32>>,
    closed: Mutex<bool>,
}

impl UnixConnection {
    /// Creates a new `UnixConnection`.
    ///
    /// Mirrors construction via `GSocketConnection` factory.
    pub fn new() -> Self {
        Self {
            peer_credentials: None,
            received_fd: Mutex::new(None),
            closed: Mutex::new(false),
        }
    }

    /// Creates a connection with known peer credentials.
    pub fn new_with_credentials(creds: Credentials) -> Self {
        Self {
            peer_credentials: Some(creds),
            received_fd: Mutex::new(None),
            closed: Mutex::new(false),
        }
    }

    /// Returns the peer credentials, if available.
    ///
    /// Mirrors `g_unix_connection_receive_credentials`.
    pub fn get_peer_credentials(&self) -> Option<&Credentials> {
        self.peer_credentials.as_ref()
    }

    /// Simulates receiving a file descriptor from the peer.
    ///
    /// Mirrors `g_unix_connection_receive_fd`.
    pub fn receive_fd(&self) -> Option<i32> {
        self.received_fd.lock().take()
    }

    /// Simulates sending a file descriptor to the peer.
    ///
    /// Mirrors `g_unix_connection_send_fd`.
    pub fn send_fd(&self, fd: i32) {
        *self.received_fd.lock() = Some(fd);
    }

    /// Closes the connection.
    pub fn close(&self) {
        *self.closed.lock() = true;
    }

    /// Returns true if the connection has been closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

impl Default for UnixConnection {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gcredentials::Credentials;

    #[test]
    fn test_new() {
        let c = UnixConnection::new();
        assert!(!c.is_closed());
        assert!(c.get_peer_credentials().is_none());
    }

    #[test]
    fn test_with_credentials() {
        let creds = Credentials::new_with(42, 1000, 1000);
        let c = UnixConnection::new_with_credentials(creds);
        assert_eq!(
            c.get_peer_credentials().unwrap().get_unix_pid().unwrap(),
            42
        );
    }

    #[test]
    fn test_send_receive_fd() {
        let c = UnixConnection::new();
        assert!(c.receive_fd().is_none());
        c.send_fd(7);
        assert_eq!(c.receive_fd(), Some(7));
        assert!(c.receive_fd().is_none()); // consumed
    }

    #[test]
    fn test_close() {
        let c = UnixConnection::new();
        c.close();
        assert!(c.is_closed());
    }

    #[test]
    fn test_default() {
        let c = UnixConnection::default();
        assert!(!c.is_closed());
    }
}
