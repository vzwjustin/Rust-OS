//! GTlsConnection matching `gio/gtlsconnection.h`.
//!
//! Wraps an inner IOStream and tracks TLS handshake state. In this no_std
//! port there is no real TLS stack; the type models the GIO API surface.

use spin::Mutex;

use crate::gtlscertificate::{TlsCertificate, TlsCertificateFlags};

/// A TLS connection (`GTlsConnection`).
///
/// Wraps an inner IOStream and provides TLS handshake state management.
/// Mirrors the GIO `GTlsConnection` object hierarchy.
pub struct TlsConnection {
    /// Local certificate presented to the peer.
    certificate: Option<TlsCertificate>,
    /// Remote peer's certificate, received during handshake.
    peer_certificate: Option<TlsCertificate>,
    /// Verification errors found on the peer certificate.
    peer_certificate_errors: TlsCertificateFlags,
    /// Whether a close_notify alert must be sent/received before closing.
    require_close_notify: bool,
    /// Set to `true` once `handshake()` has succeeded.
    handshake_done: Mutex<bool>,
    /// Set to `true` once `close()` is called.
    closed: Mutex<bool>,
}

impl TlsConnection {
    /// Creates a new `TlsConnection` with no certificate and handshake not done.
    ///
    /// Mirrors `g_tls_connection_new` (simplified: no underlying IOStream).
    pub fn new() -> Self {
        Self {
            certificate: None,
            peer_certificate: None,
            peer_certificate_errors: TlsCertificateFlags::NO_FLAGS,
            require_close_notify: true,
            handshake_done: Mutex::new(false),
            closed: Mutex::new(false),
        }
    }

    // -------------------------------------------------------------------------
    // Local certificate
    // -------------------------------------------------------------------------

    /// Sets the local certificate to present to the peer.
    ///
    /// Mirrors `g_tls_connection_set_certificate`.
    pub fn set_certificate(&mut self, cert: TlsCertificate) {
        self.certificate = Some(cert);
    }

    /// Returns the local certificate, if one has been set.
    ///
    /// Mirrors `g_tls_connection_get_certificate`.
    pub fn get_certificate(&self) -> Option<&TlsCertificate> {
        self.certificate.as_ref()
    }

    // -------------------------------------------------------------------------
    // Peer certificate
    // -------------------------------------------------------------------------

    /// Returns the peer's certificate received during the handshake.
    ///
    /// Mirrors `g_tls_connection_get_peer_certificate`.
    pub fn get_peer_certificate(&self) -> Option<&TlsCertificate> {
        self.peer_certificate.as_ref()
    }

    /// Sets the peer certificate and its verification errors.
    ///
    /// This is a platform/test hook: real implementations fill this during the
    /// TLS handshake. Mirrors `g_tls_connection_get_peer_certificate_errors`.
    pub fn set_peer_certificate(&mut self, cert: TlsCertificate, errors: TlsCertificateFlags) {
        self.peer_certificate = Some(cert);
        self.peer_certificate_errors = errors;
    }

    /// Returns the verification errors found on the peer certificate.
    ///
    /// Mirrors `g_tls_connection_get_peer_certificate_errors`.
    pub fn get_peer_certificate_errors(&self) -> TlsCertificateFlags {
        self.peer_certificate_errors
    }

    // -------------------------------------------------------------------------
    // Close-notify
    // -------------------------------------------------------------------------

    /// Sets whether a TLS close_notify alert is required before closing.
    ///
    /// Mirrors `g_tls_connection_set_require_close_notify`.
    pub fn set_require_close_notify(&mut self, v: bool) {
        self.require_close_notify = v;
    }

    /// Returns whether a close_notify alert is required before closing.
    ///
    /// Mirrors `g_tls_connection_get_require_close_notify`.
    pub fn get_require_close_notify(&self) -> bool {
        self.require_close_notify
    }

    // -------------------------------------------------------------------------
    // Handshake
    // -------------------------------------------------------------------------

    /// Performs (or re-performs) the TLS handshake.
    ///
    /// Sets `handshake_done` to `true` on success. Returns `Err(())` if the
    /// connection is already closed.
    ///
    /// Mirrors `g_tls_connection_handshake`.
    pub fn handshake(&self) -> Result<(), ()> {
        if *self.closed.lock() {
            return Err(());
        }
        *self.handshake_done.lock() = true;
        Ok(())
    }

    /// Returns `true` if a successful handshake has been completed.
    pub fn is_handshake_done(&self) -> bool {
        *self.handshake_done.lock()
    }

    // -------------------------------------------------------------------------
    // Close
    // -------------------------------------------------------------------------

    /// Closes the TLS connection.
    ///
    /// Mirrors `g_tls_connection_close` / `g_io_stream_close`.
    pub fn close(&self) {
        *self.closed.lock() = true;
    }

    /// Returns `true` if the connection has been closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

impl Default for TlsConnection {
    fn default() -> Self {
        Self::new()
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_PEM: &[u8] = b"-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----\n";

    fn make_cert() -> TlsCertificate {
        TlsCertificate::new_from_pem(FAKE_PEM)
    }

    #[test]
    fn test_new_defaults() {
        let conn = TlsConnection::new();
        assert!(conn.get_certificate().is_none());
        assert!(conn.get_peer_certificate().is_none());
        assert!(!conn.is_handshake_done());
        assert!(!conn.is_closed());
        assert!(conn.get_require_close_notify());
        assert_eq!(
            conn.get_peer_certificate_errors(),
            TlsCertificateFlags::NO_FLAGS
        );
    }

    #[test]
    fn test_set_get_certificate() {
        let mut conn = TlsConnection::new();
        conn.set_certificate(make_cert());
        assert!(conn.get_certificate().is_some());
        assert_eq!(conn.get_certificate().unwrap().get_pem(), FAKE_PEM);
    }

    #[test]
    fn test_handshake_sets_done() {
        let conn = TlsConnection::new();
        assert!(!conn.is_handshake_done());
        conn.handshake().expect("handshake should succeed");
        assert!(conn.is_handshake_done());
    }

    #[test]
    fn test_handshake_fails_when_closed() {
        let conn = TlsConnection::new();
        conn.close();
        assert!(conn.handshake().is_err());
        assert!(!conn.is_handshake_done());
    }

    #[test]
    fn test_close_sets_closed() {
        let conn = TlsConnection::new();
        assert!(!conn.is_closed());
        conn.close();
        assert!(conn.is_closed());
    }

    #[test]
    fn test_peer_certificate_and_errors() {
        let mut conn = TlsConnection::new();
        let cert = make_cert();
        let errors = TlsCertificateFlags::EXPIRED | TlsCertificateFlags::UNKNOWN_CA;
        conn.set_peer_certificate(cert, errors);
        assert!(conn.get_peer_certificate().is_some());
        assert!(conn
            .get_peer_certificate_errors()
            .contains(TlsCertificateFlags::EXPIRED));
        assert!(conn
            .get_peer_certificate_errors()
            .contains(TlsCertificateFlags::UNKNOWN_CA));
        assert!(!conn
            .get_peer_certificate_errors()
            .contains(TlsCertificateFlags::REVOKED));
    }

    #[test]
    fn test_require_close_notify_toggle() {
        let mut conn = TlsConnection::new();
        assert!(conn.get_require_close_notify()); // default true
        conn.set_require_close_notify(false);
        assert!(!conn.get_require_close_notify());
        conn.set_require_close_notify(true);
        assert!(conn.get_require_close_notify());
    }

    #[test]
    fn test_handshake_idempotent() {
        // Calling handshake twice on an open connection is fine.
        let conn = TlsConnection::new();
        conn.handshake().unwrap();
        conn.handshake().unwrap();
        assert!(conn.is_handshake_done());
    }
}
