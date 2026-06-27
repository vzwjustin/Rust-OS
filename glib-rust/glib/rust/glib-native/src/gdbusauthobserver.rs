//! GDBusAuthObserver matching `gio/gdbusauthobserver.h`.
//!
//! An observer for D-Bus authentication. In this no_std port we model
//! allowed mechanisms and peer authorization.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A D-Bus auth observer (`GDBusAuthObserver`).
pub struct DBusAuthObserver {
    allowed_mechanisms: Mutex<Vec<String>>,
    authorized_peers: Mutex<Vec<String>>,
}

impl DBusAuthObserver {
    /// Creates a new auth observer.
    ///
    /// Mirrors `g_dbus_auth_observer_new`.
    pub fn new() -> Self {
        Self {
            allowed_mechanisms: Mutex::new(Vec::new()),
            authorized_peers: Mutex::new(Vec::new()),
        }
    }

    /// Checks if a mechanism is allowed.
    ///
    /// Mirrors `g_dbus_auth_observer_allow_mechanism`. When no mechanisms have
    /// been configured yet, all mechanisms are allowed.
    pub fn allow_mechanism(&self, mechanism: &str) -> bool {
        let mechs = self.allowed_mechanisms.lock();
        mechs.is_empty() || mechs.iter().any(|m| m == mechanism)
    }

    /// Restricts authentication to the given mechanism names.
    pub fn set_allowed_mechanisms(&self, mechanisms: &[&str]) {
        let mut mechs = self.allowed_mechanisms.lock();
        mechs.clear();
        for m in mechanisms {
            mechs.push((*m).to_string());
        }
    }

    /// Returns the list of allowed mechanisms.
    pub fn get_allowed_mechanisms(&self) -> Vec<String> {
        self.allowed_mechanisms.lock().clone()
    }

    /// Authorizes an authenticated peer.
    ///
    /// Mirrors `g_dbus_auth_observer_authorize_authenticated_peer`.
    pub fn authorize_authenticated_peer(&self, peer_id: &str) -> bool {
        self.authorized_peers.lock().push(peer_id.to_string());
        true
    }

    /// Returns the list of authorized peers.
    pub fn get_authorized_peers(&self) -> Vec<String> {
        self.authorized_peers.lock().clone()
    }

    /// Checks if a peer has been authorized.
    pub fn is_peer_authorized(&self, peer_id: &str) -> bool {
        self.authorized_peers.lock().iter().any(|p| p == peer_id)
    }
}

impl Default for DBusAuthObserver {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let obs = DBusAuthObserver::new();
        assert!(obs.get_allowed_mechanisms().is_empty());
        assert!(obs.get_authorized_peers().is_empty());
    }

    #[test]
    fn test_allow_mechanism() {
        let obs = DBusAuthObserver::new();
        assert!(obs.allow_mechanism("ANONYMOUS"));
        obs.set_allowed_mechanisms(&["EXTERNAL"]);
        assert!(!obs.allow_mechanism("ANONYMOUS"));
        assert!(obs.allow_mechanism("EXTERNAL"));
    }

    #[test]
    fn test_authorize_peer() {
        let obs = DBusAuthObserver::new();
        assert!(obs.authorize_authenticated_peer(":1.42"));
        assert!(obs.is_peer_authorized(":1.42"));
        assert!(!obs.is_peer_authorized(":1.99"));
    }

    #[test]
    fn test_default() {
        let obs = DBusAuthObserver::default();
        assert!(obs.get_allowed_mechanisms().is_empty());
    }
}
