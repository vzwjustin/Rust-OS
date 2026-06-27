//! `GDBusServer` — no_std port of GIO's D-Bus server helper.
//!
//! Models `GDBusServer` from `gio/gdbusserver.h`. Manages a D-Bus server
//! address, GUID, active state, and the set of connected peer addresses.
//!
//! Fully `no_std` compatible: uses `alloc` types and `spin::Mutex` for
//! interior mutability.

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::BitOr;
use spin::Mutex;

// ── Flags ─────────────────────────────────────────────────────────────────────

/// Flags controlling how a [`DBusServer`] behaves.
///
/// Mirrors `GDBusServerFlags` in `gio/gdbusserver.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DBusServerFlags(pub u32);

impl DBusServerFlags {
    /// No special flags.
    pub const NONE: DBusServerFlags = DBusServerFlags(0);
    /// Each incoming connection is handled in its own thread.
    pub const RUN_IN_THREAD: DBusServerFlags = DBusServerFlags(1);
    /// Allow anonymous (unauthenticated) clients to connect.
    pub const AUTHENTICATION_ALLOW_ANONYMOUS: DBusServerFlags = DBusServerFlags(2);
    /// Require that the connecting user is the same as the server user.
    pub const AUTHENTICATION_REQUIRE_SAME_USER: DBusServerFlags = DBusServerFlags(4);

    /// Returns `true` if all bits of `other` are set in `self`.
    #[inline]
    pub fn contains(self, other: DBusServerFlags) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl BitOr for DBusServerFlags {
    type Output = DBusServerFlags;
    #[inline]
    fn bitor(self, rhs: DBusServerFlags) -> DBusServerFlags {
        DBusServerFlags(self.0 | rhs.0)
    }
}

impl Default for DBusServerFlags {
    fn default() -> Self {
        DBusServerFlags::NONE
    }
}

// ── Server ─────────────────────────────────────────────────────────────────────

/// A D-Bus server that accepts incoming connections on a given address.
///
/// Mirrors `GDBusServer` from `gio/gdbusserver.h`.
pub struct DBusServer {
    /// The D-Bus address string this server listens on (e.g. `unix:path=/tmp/dbus-foo`).
    address: String,
    /// Behaviour flags passed at construction.
    flags: DBusServerFlags,
    /// The server's globally-unique identifier (UUID).
    guid: String,
    /// Whether the server is currently running.
    active: Mutex<bool>,
    /// Number of accepted client connections (len-mirror of `connections`).
    client_count: Mutex<u32>,
    /// Peer addresses of currently-connected clients.
    connections: Mutex<Vec<String>>,
}

impl DBusServer {
    /// Creates a new [`DBusServer`] with [`DBusServerFlags::NONE`].
    ///
    /// Mirrors `g_dbus_server_new_sync` (simplified: no auth observer / cancellable).
    pub fn new(address: &str, guid: &str) -> Self {
        Self::new_with_flags(address, guid, DBusServerFlags::NONE)
    }

    /// Creates a new [`DBusServer`] with the given `flags`.
    pub fn new_with_flags(address: &str, guid: &str, flags: DBusServerFlags) -> Self {
        DBusServer {
            address: String::from(address),
            flags,
            guid: String::from(guid),
            active: Mutex::new(false),
            client_count: Mutex::new(0),
            connections: Mutex::new(Vec::new()),
        }
    }

    // ── Getters ───────────────────────────────────────────────────────────────

    /// Returns the D-Bus address this server is listening on.
    ///
    /// Mirrors `g_dbus_server_get_client_address`.
    pub fn get_client_address(&self) -> &str {
        &self.address
    }

    /// Returns the server's GUID.
    ///
    /// Mirrors `g_dbus_server_get_guid`.
    pub fn get_guid(&self) -> &str {
        &self.guid
    }

    /// Returns the flags the server was constructed with.
    ///
    /// Mirrors `g_dbus_server_get_flags`.
    pub fn get_flags(&self) -> DBusServerFlags {
        self.flags
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Starts accepting connections.
    ///
    /// Mirrors `g_dbus_server_start`.
    pub fn start(&self) {
        *self.active.lock() = true;
    }

    /// Stops accepting connections.
    ///
    /// Mirrors `g_dbus_server_stop`.
    pub fn stop(&self) {
        *self.active.lock() = false;
    }

    /// Returns `true` if the server is currently running.
    ///
    /// Mirrors `g_dbus_server_is_active`.
    pub fn is_active(&self) -> bool {
        *self.active.lock()
    }

    /// Returns `true` if the server is accepting new connections.
    ///
    /// Equivalent to [`is_active`](DBusServer::is_active) — GIO checks the
    /// same flag for both concepts.
    pub fn is_accepting_connections(&self) -> bool {
        self.is_active()
    }

    // ── Connection management ──────────────────────────────────────────────────

    /// Registers an incoming connection from `peer_addr`.
    ///
    /// Returns `true` if the connection was accepted (i.e. the server is
    /// active). Returns `false` when the server is stopped.
    pub fn accept_connection(&self, peer_addr: &str) -> bool {
        if !self.is_active() {
            return false;
        }
        let mut conns = self.connections.lock();
        conns.push(String::from(peer_addr));
        *self.client_count.lock() = conns.len() as u32;
        true
    }

    /// Removes the first connection whose peer address equals `peer_addr`.
    ///
    /// Returns `true` if a matching connection was found and removed.
    pub fn disconnect_client(&self, peer_addr: &str) -> bool {
        let mut conns = self.connections.lock();
        if let Some(pos) = conns.iter().position(|c| c == peer_addr) {
            conns.remove(pos);
            *self.client_count.lock() = conns.len() as u32;
            true
        } else {
            false
        }
    }

    /// Returns the number of currently active connections.
    pub fn active_connections(&self) -> u32 {
        self.connections.lock().len() as u32
    }
}

impl Default for DBusServer {
    fn default() -> Self {
        DBusServer {
            address: String::new(),
            flags: DBusServerFlags::NONE,
            guid: String::new(),
            active: Mutex::new(false),
            client_count: Mutex::new(0),
            connections: Mutex::new(Vec::new()),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_stores_address_and_guid() {
        let srv = DBusServer::new("unix:path=/tmp/test", "test-guid-1234");
        assert_eq!(srv.get_client_address(), "unix:path=/tmp/test");
        assert_eq!(srv.get_guid(), "test-guid-1234");
    }

    #[test]
    fn test_default_flags_are_none() {
        let srv = DBusServer::new("unix:abstract=foo", "guid-abc");
        assert_eq!(srv.get_flags(), DBusServerFlags::NONE);
    }

    #[test]
    fn test_new_with_flags() {
        let flags =
            DBusServerFlags::RUN_IN_THREAD | DBusServerFlags::AUTHENTICATION_ALLOW_ANONYMOUS;
        let srv = DBusServer::new_with_flags("tcp:host=localhost,port=12345", "guid-xyz", flags);
        assert!(srv.get_flags().contains(DBusServerFlags::RUN_IN_THREAD));
        assert!(srv
            .get_flags()
            .contains(DBusServerFlags::AUTHENTICATION_ALLOW_ANONYMOUS));
        assert!(!srv
            .get_flags()
            .contains(DBusServerFlags::AUTHENTICATION_REQUIRE_SAME_USER));
    }

    #[test]
    fn test_start_and_stop() {
        let srv = DBusServer::new("unix:path=/tmp/s", "g1");
        assert!(!srv.is_active());
        srv.start();
        assert!(srv.is_active());
        assert!(srv.is_accepting_connections());
        srv.stop();
        assert!(!srv.is_active());
        assert!(!srv.is_accepting_connections());
    }

    #[test]
    fn test_accept_connection_requires_active() {
        let srv = DBusServer::new("unix:path=/tmp/s2", "g2");
        // Server is stopped — should reject.
        assert!(!srv.accept_connection("peer-1"));
        assert_eq!(srv.active_connections(), 0);
        // Start and try again.
        srv.start();
        assert!(srv.accept_connection("peer-1"));
        assert_eq!(srv.active_connections(), 1);
    }

    #[test]
    fn test_multiple_connections() {
        let srv = DBusServer::new("unix:path=/tmp/s3", "g3");
        srv.start();
        assert!(srv.accept_connection("peer-A"));
        assert!(srv.accept_connection("peer-B"));
        assert!(srv.accept_connection("peer-C"));
        assert_eq!(srv.active_connections(), 3);
    }

    #[test]
    fn test_disconnect_client_found() {
        let srv = DBusServer::new("unix:path=/tmp/s4", "g4");
        srv.start();
        srv.accept_connection("peer-X");
        srv.accept_connection("peer-Y");
        assert!(srv.disconnect_client("peer-X"));
        assert_eq!(srv.active_connections(), 1);
    }

    #[test]
    fn test_disconnect_client_not_found() {
        let srv = DBusServer::new("unix:path=/tmp/s5", "g5");
        srv.start();
        srv.accept_connection("peer-1");
        assert!(!srv.disconnect_client("peer-99"));
        assert_eq!(srv.active_connections(), 1);
    }

    #[test]
    fn test_default_impl() {
        let srv = DBusServer::default();
        assert_eq!(srv.get_client_address(), "");
        assert_eq!(srv.get_guid(), "");
        assert_eq!(srv.get_flags(), DBusServerFlags::NONE);
        assert!(!srv.is_active());
        assert_eq!(srv.active_connections(), 0);
    }

    #[test]
    fn test_flags_contains_and_bitor() {
        let f = DBusServerFlags::AUTHENTICATION_ALLOW_ANONYMOUS
            | DBusServerFlags::AUTHENTICATION_REQUIRE_SAME_USER;
        assert!(f.contains(DBusServerFlags::AUTHENTICATION_ALLOW_ANONYMOUS));
        assert!(f.contains(DBusServerFlags::AUTHENTICATION_REQUIRE_SAME_USER));
        assert!(!f.contains(DBusServerFlags::RUN_IN_THREAD));
        // NONE is always contained (0 bits are trivially a subset).
        assert!(f.contains(DBusServerFlags::NONE));
    }
}
