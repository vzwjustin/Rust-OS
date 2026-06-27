//! GTlsClientConnection matching `gio/gtlsclientconnection.h`.
//!
//! TLS client-side connection. In this no_std port we model the
//! connection state with server identity and validation flags.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gtlscertificate::TlsCertificateFlags;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A TLS client connection (`GTlsClientConnection`).
pub struct TlsClientConnection {
    server_identity: Mutex<Option<String>>,
    validation_flags: Mutex<TlsCertificateFlags>,
    accepted_cas: Mutex<Vec<String>>,
    closed: Mutex<bool>,
}

impl TlsClientConnection {
    /// Creates a new TLS client connection.
    ///
    /// Mirrors `g_tls_client_connection_new`.
    pub fn new(server_identity: Option<&str>) -> Self {
        Self {
            server_identity: Mutex::new(server_identity.map(|s| s.to_string())),
            validation_flags: Mutex::new(TlsCertificateFlags::NO_FLAGS),
            accepted_cas: Mutex::new(Vec::new()),
            closed: Mutex::new(false),
        }
    }

    /// Gets the server identity.
    ///
    /// Mirrors `g_tls_client_connection_get_server_identity`.
    pub fn get_server_identity(&self) -> Option<String> {
        self.server_identity.lock().clone()
    }

    /// Sets the server identity.
    ///
    /// Mirrors `g_tls_client_connection_set_server_identity`.
    pub fn set_server_identity(&self, identity: &str) {
        *self.server_identity.lock() = Some(identity.to_string());
    }

    /// Gets the validation flags.
    ///
    /// Mirrors `g_tls_client_connection_get_validation_flags`.
    pub fn get_validation_flags(&self) -> TlsCertificateFlags {
        *self.validation_flags.lock()
    }

    /// Sets the validation flags.
    ///
    /// Mirrors `g_tls_client_connection_set_validation_flags`.
    pub fn set_validation_flags(&self, flags: TlsCertificateFlags) {
        *self.validation_flags.lock() = flags;
    }

    /// Gets the list of accepted certificate authorities.
    ///
    /// Mirrors `g_tls_client_connection_get_accepted_cas`.
    pub fn get_accepted_cas(&self) -> Vec<String> {
        self.accepted_cas.lock().clone()
    }

    /// Adds an accepted CA.
    pub fn add_accepted_ca(&self, ca: &str) {
        self.accepted_cas.lock().push(ca.to_string());
    }

    /// Copies session state from another connection.
    ///
    /// Mirrors `g_tls_client_connection_copy_session_state`.
    pub fn copy_session_state(&self, source: &TlsClientConnection) {
        *self.server_identity.lock() = source.server_identity.lock().clone();
        *self.validation_flags.lock() = *source.validation_flags.lock();
    }

    /// Closes the connection.
    pub fn close(&self) {
        *self.closed.lock() = true;
    }

    /// Returns whether the connection is closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let conn = TlsClientConnection::new(Some("example.com"));
        assert_eq!(conn.get_server_identity().unwrap(), "example.com");
        assert!(!conn.is_closed());
    }

    #[test]
    fn test_new_no_identity() {
        let conn = TlsClientConnection::new(None);
        assert!(conn.get_server_identity().is_none());
    }

    #[test]
    fn test_set_server_identity() {
        let conn = TlsClientConnection::new(None);
        conn.set_server_identity("test.org");
        assert_eq!(conn.get_server_identity().unwrap(), "test.org");
    }

    #[test]
    fn test_validation_flags() {
        let conn = TlsClientConnection::new(None);
        conn.set_validation_flags(TlsCertificateFlags::EXPIRED);
        assert!(conn
            .get_validation_flags()
            .contains(TlsCertificateFlags::EXPIRED));
    }

    #[test]
    fn test_accepted_cas() {
        let conn = TlsClientConnection::new(None);
        conn.add_accepted_ca("CN=CA1");
        conn.add_accepted_ca("CN=CA2");
        let cas = conn.get_accepted_cas();
        assert_eq!(cas.len(), 2);
        assert_eq!(cas[0], "CN=CA1");
    }

    #[test]
    fn test_copy_session_state() {
        let src = TlsClientConnection::new(Some("source.com"));
        src.set_validation_flags(TlsCertificateFlags::UNKNOWN_CA);
        let dst = TlsClientConnection::new(None);
        dst.copy_session_state(&src);
        assert_eq!(dst.get_server_identity().unwrap(), "source.com");
        assert!(dst
            .get_validation_flags()
            .contains(TlsCertificateFlags::UNKNOWN_CA));
    }

    #[test]
    fn test_close() {
        let conn = TlsClientConnection::new(None);
        conn.close();
        assert!(conn.is_closed());
    }
}
