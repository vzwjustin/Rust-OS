//! GDtlsConnection matching `gio/gdtlsconnection.h`.
//!
//! Represents a DTLS (Datagram TLS) connection — TLS over UDP.  Similar to
//! `GTlsConnection` but datagram-oriented.  This no_std port models the
//! connection state and certificate fields; actual cryptographic I/O is left
//! to a platform layer.

use crate::gtlscertificate::{TlsCertificate, TlsCertificateFlags};
use spin::Mutex;

// ──────────────────────────────────────────────────────────────────────────────
// RehandshakeMode
// ──────────────────────────────────────────────────────────────────────────────

/// Controls how (or whether) the connection may be re-handshaked.
///
/// Mirrors `GTlsRehandshakeMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RehandshakeMode {
    /// Never allow re-handshaking.
    Never,
    /// Allow safe re-handshaking only.
    Safely,
    /// Allow unsafe re-handshaking.
    Unsafely,
}

// ──────────────────────────────────────────────────────────────────────────────
// DtlsConnection
// ──────────────────────────────────────────────────────────────────────────────

/// A DTLS connection (`GDtlsConnection`).
///
/// Owns the local certificate and tracks the peer's certificate plus any
/// verification errors raised during the handshake.
pub struct DtlsConnection {
    certificate: Option<TlsCertificate>,
    peer_certificate: Option<TlsCertificate>,
    peer_certificate_errors: TlsCertificateFlags,
    require_close_notify: bool,
    rehandshake_mode: RehandshakeMode,
    handshake_done: Mutex<bool>,
    closed: Mutex<bool>,
}

impl DtlsConnection {
    /// Creates a new `DtlsConnection` with default settings.
    ///
    /// `require_close_notify` defaults to `true`; `rehandshake_mode` defaults
    /// to `RehandshakeMode::Safely`, matching GIO's defaults.
    pub fn new() -> Self {
        Self {
            certificate: None,
            peer_certificate: None,
            peer_certificate_errors: TlsCertificateFlags::NO_FLAGS,
            require_close_notify: true,
            rehandshake_mode: RehandshakeMode::Safely,
            handshake_done: Mutex::new(false),
            closed: Mutex::new(false),
        }
    }

    // ── Local certificate ────────────────────────────────────────────────────

    /// Sets the local TLS certificate used by this connection.
    ///
    /// Mirrors `g_dtls_connection_set_certificate`.
    pub fn set_certificate(&mut self, cert: TlsCertificate) {
        self.certificate = Some(cert);
    }

    /// Returns a reference to the local certificate, if set.
    ///
    /// Mirrors `g_dtls_connection_get_certificate`.
    pub fn get_certificate(&self) -> Option<&TlsCertificate> {
        self.certificate.as_ref()
    }

    // ── Peer certificate ─────────────────────────────────────────────────────

    /// Returns the peer's certificate as presented during the handshake.
    ///
    /// Mirrors `g_dtls_connection_get_peer_certificate`.
    pub fn get_peer_certificate(&self) -> Option<&TlsCertificate> {
        self.peer_certificate.as_ref()
    }

    /// Sets the peer certificate and associated errors (test / platform hook).
    pub fn set_peer_certificate(&mut self, cert: TlsCertificate, errors: TlsCertificateFlags) {
        self.peer_certificate = Some(cert);
        self.peer_certificate_errors = errors;
    }

    /// Returns the verification errors for the peer's certificate.
    ///
    /// Mirrors `g_dtls_connection_get_peer_certificate_errors`.
    pub fn get_peer_certificate_errors(&self) -> TlsCertificateFlags {
        self.peer_certificate_errors
    }

    // ── Rehandshake mode ─────────────────────────────────────────────────────

    /// Returns the current rehandshake mode.
    ///
    /// Mirrors `g_dtls_connection_get_rehandshake_mode`.
    pub fn get_rehandshake_mode(&self) -> RehandshakeMode {
        self.rehandshake_mode
    }

    /// Sets the rehandshake mode.
    ///
    /// Mirrors `g_dtls_connection_set_rehandshake_mode`.
    pub fn set_rehandshake_mode(&mut self, mode: RehandshakeMode) {
        self.rehandshake_mode = mode;
    }

    // ── Close-notify ─────────────────────────────────────────────────────────

    /// Returns whether a close-notify alert is required before closing.
    ///
    /// Mirrors `g_dtls_connection_get_require_close_notify`.
    pub fn get_require_close_notify(&self) -> bool {
        self.require_close_notify
    }

    /// Sets whether a close-notify alert is required before closing.
    ///
    /// Mirrors `g_dtls_connection_set_require_close_notify`.
    pub fn set_require_close_notify(&mut self, require: bool) {
        self.require_close_notify = require;
    }

    // ── Handshake ────────────────────────────────────────────────────────────

    /// Performs (or completes) the DTLS handshake.
    ///
    /// Returns `Err(())` if the connection is already closed.  On success the
    /// `handshake_done` flag is set to `true`.
    ///
    /// Mirrors `g_dtls_connection_handshake`.
    pub fn handshake(&self) -> Result<(), ()> {
        if *self.closed.lock() {
            return Err(());
        }
        *self.handshake_done.lock() = true;
        Ok(())
    }

    /// Returns `true` if the handshake has completed successfully.
    pub fn is_handshake_done(&self) -> bool {
        *self.handshake_done.lock()
    }

    // ── Close ────────────────────────────────────────────────────────────────

    /// Closes the DTLS connection.
    ///
    /// Mirrors `g_dtls_connection_close`.
    pub fn close(&self) {
        *self.closed.lock() = true;
    }

    /// Returns `true` if the connection has been closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

impl Default for DtlsConnection {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_PEM: &[u8] = b"-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----\n";

    fn make_cert() -> TlsCertificate {
        TlsCertificate::new_from_pem(FAKE_PEM)
    }

    #[test]
    fn test_new_defaults() {
        let conn = DtlsConnection::new();
        assert!(conn.get_certificate().is_none());
        assert!(conn.get_peer_certificate().is_none());
        assert_eq!(
            conn.get_peer_certificate_errors(),
            TlsCertificateFlags::NO_FLAGS
        );
        assert!(conn.get_require_close_notify());
        assert_eq!(conn.get_rehandshake_mode(), RehandshakeMode::Safely);
        assert!(!conn.is_handshake_done());
        assert!(!conn.is_closed());
    }

    #[test]
    fn test_set_get_certificate() {
        let mut conn = DtlsConnection::new();
        conn.set_certificate(make_cert());
        assert!(conn.get_certificate().is_some());
        assert_eq!(conn.get_certificate().unwrap().get_pem(), FAKE_PEM);
    }

    #[test]
    fn test_set_get_peer_certificate() {
        let mut conn = DtlsConnection::new();
        let errors = TlsCertificateFlags::EXPIRED | TlsCertificateFlags::BAD_IDENTITY;
        conn.set_peer_certificate(make_cert(), errors);
        assert!(conn.get_peer_certificate().is_some());
        assert!(conn
            .get_peer_certificate_errors()
            .contains(TlsCertificateFlags::EXPIRED));
        assert!(conn
            .get_peer_certificate_errors()
            .contains(TlsCertificateFlags::BAD_IDENTITY));
    }

    #[test]
    fn test_handshake_success() {
        let conn = DtlsConnection::new();
        assert!(conn.handshake().is_ok());
        assert!(conn.is_handshake_done());
    }

    #[test]
    fn test_handshake_fails_when_closed() {
        let conn = DtlsConnection::new();
        conn.close();
        assert!(conn.is_closed());
        assert!(conn.handshake().is_err());
        // handshake_done should remain false
        assert!(!conn.is_handshake_done());
    }

    #[test]
    fn test_close_and_is_closed() {
        let conn = DtlsConnection::new();
        assert!(!conn.is_closed());
        conn.close();
        assert!(conn.is_closed());
    }

    #[test]
    fn test_rehandshake_mode() {
        let mut conn = DtlsConnection::new();
        assert_eq!(conn.get_rehandshake_mode(), RehandshakeMode::Safely);
        conn.set_rehandshake_mode(RehandshakeMode::Never);
        assert_eq!(conn.get_rehandshake_mode(), RehandshakeMode::Never);
        conn.set_rehandshake_mode(RehandshakeMode::Unsafely);
        assert_eq!(conn.get_rehandshake_mode(), RehandshakeMode::Unsafely);
    }

    #[test]
    fn test_require_close_notify() {
        let mut conn = DtlsConnection::new();
        assert!(conn.get_require_close_notify());
        conn.set_require_close_notify(false);
        assert!(!conn.get_require_close_notify());
        conn.set_require_close_notify(true);
        assert!(conn.get_require_close_notify());
    }
}
