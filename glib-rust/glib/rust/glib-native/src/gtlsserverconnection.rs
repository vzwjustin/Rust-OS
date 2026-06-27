//! GTlsServerConnection matching `gio/gtlsserverconnection.h`.
//!
//! TLS server-side connection. In this no_std port we model the
//! connection state with certificate, peer certificate, authentication
//! mode, and handshake/closed flags.
//!
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::gtlscertificate::{TlsCertificate, TlsCertificateFlags};
use spin::Mutex;

// ──────────────────────────── ClientCertificateMode ───────────────────────

/// Controls whether a TLS server requests or requires a client certificate
/// (`GTlsAuthenticationMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientCertificateMode {
    /// Never request a client certificate.
    Never,
    /// Request a client certificate but do not require one.
    Requested,
    /// Require a valid client certificate.
    Required,
}

// ──────────────────────────── TlsServerConnection ─────────────────────────

/// A TLS server-side connection (`GTlsServerConnection`).
pub struct TlsServerConnection {
    certificate: Option<TlsCertificate>,
    authentication_mode: Mutex<ClientCertificateMode>,
    peer_certificate: Option<TlsCertificate>,
    peer_certificate_errors: Mutex<TlsCertificateFlags>,
    handshake_done: Mutex<bool>,
    closed: Mutex<bool>,
}

impl TlsServerConnection {
    /// Creates a new, unconfigured TLS server connection.
    ///
    /// Mirrors `g_tls_server_connection_new` (no-argument form).
    pub fn new() -> Self {
        Self {
            certificate: None,
            authentication_mode: Mutex::new(ClientCertificateMode::Never),
            peer_certificate: None,
            peer_certificate_errors: Mutex::new(TlsCertificateFlags::NO_FLAGS),
            handshake_done: Mutex::new(false),
            closed: Mutex::new(false),
        }
    }

    /// Creates a new TLS server connection with a pre-loaded server certificate.
    ///
    /// Mirrors `g_tls_server_connection_new` with a `certificate` argument.
    pub fn new_with_certificate(cert: TlsCertificate) -> Self {
        Self {
            certificate: Some(cert),
            authentication_mode: Mutex::new(ClientCertificateMode::Never),
            peer_certificate: None,
            peer_certificate_errors: Mutex::new(TlsCertificateFlags::NO_FLAGS),
            handshake_done: Mutex::new(false),
            closed: Mutex::new(false),
        }
    }

    // ── Certificate ────────────────────────────────────────────────────────

    /// Sets the server certificate.
    ///
    /// Mirrors `g_tls_connection_set_certificate`.
    pub fn set_certificate(&mut self, cert: TlsCertificate) {
        self.certificate = Some(cert);
    }

    /// Returns the server certificate, if one has been configured.
    ///
    /// Mirrors `g_tls_connection_get_certificate`.
    pub fn get_certificate(&self) -> Option<&TlsCertificate> {
        self.certificate.as_ref()
    }

    // ── Authentication mode ────────────────────────────────────────────────

    /// Returns the current client-certificate authentication mode.
    ///
    /// Mirrors `g_tls_server_connection_get_authentication_mode`.
    pub fn get_authentication_mode(&self) -> ClientCertificateMode {
        *self.authentication_mode.lock()
    }

    /// Sets the client-certificate authentication mode.
    ///
    /// Mirrors `g_tls_server_connection_set_authentication_mode`.
    pub fn set_authentication_mode(&self, mode: ClientCertificateMode) {
        *self.authentication_mode.lock() = mode;
    }

    // ── Handshake ──────────────────────────────────────────────────────────

    /// Performs the TLS handshake.
    ///
    /// Returns `Err(())` if the connection has already been closed.
    /// On success, marks the handshake as done.
    ///
    /// Mirrors `g_tls_connection_handshake`.
    pub fn handshake(&self) -> Result<(), ()> {
        if *self.closed.lock() {
            return Err(());
        }
        *self.handshake_done.lock() = true;
        Ok(())
    }

    /// Returns `true` if the TLS handshake has completed successfully.
    pub fn is_handshake_done(&self) -> bool {
        *self.handshake_done.lock()
    }

    // ── Peer certificate ───────────────────────────────────────────────────

    /// Returns the peer (client) certificate, if one was received.
    ///
    /// Mirrors `g_tls_connection_get_peer_certificate`.
    pub fn get_peer_certificate(&self) -> Option<&TlsCertificate> {
        self.peer_certificate.as_ref()
    }

    /// Sets the peer (client) certificate.
    pub fn set_peer_certificate(&mut self, cert: TlsCertificate) {
        self.peer_certificate = Some(cert);
    }

    /// Returns the verification errors found on the peer certificate.
    ///
    /// Mirrors `g_tls_connection_get_peer_certificate_errors`.
    pub fn get_peer_certificate_errors(&self) -> TlsCertificateFlags {
        *self.peer_certificate_errors.lock()
    }

    // ── Close ──────────────────────────────────────────────────────────────

    /// Closes the connection.
    ///
    /// Mirrors `g_io_stream_close`.
    pub fn close(&self) {
        *self.closed.lock() = true;
    }

    /// Returns `true` if the connection has been closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
}

// ──────────────────────────── Default ─────────────────────────────────────

impl Default for TlsServerConnection {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtlscertificate::TlsCertificateFlags;

    const FAKE_PEM: &[u8] = b"-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----\n";

    fn make_cert() -> TlsCertificate {
        TlsCertificate::new_from_pem(FAKE_PEM)
    }

    // 1. new() starts in a sane, unconfigured state.
    #[test]
    fn test_new_defaults() {
        let conn = TlsServerConnection::new();
        assert!(conn.get_certificate().is_none());
        assert_eq!(conn.get_authentication_mode(), ClientCertificateMode::Never);
        assert!(conn.get_peer_certificate().is_none());
        assert_eq!(
            conn.get_peer_certificate_errors().0,
            TlsCertificateFlags::NO_FLAGS.0
        );
        assert!(!conn.is_handshake_done());
        assert!(!conn.is_closed());
    }

    // 2. Default trait delegates to new().
    #[test]
    fn test_default() {
        let conn = TlsServerConnection::default();
        assert!(conn.get_certificate().is_none());
        assert_eq!(conn.get_authentication_mode(), ClientCertificateMode::Never);
        assert!(!conn.is_closed());
    }

    // 3. new_with_certificate() pre-loads the certificate.
    #[test]
    fn test_new_with_certificate() {
        let cert = make_cert();
        let conn = TlsServerConnection::new_with_certificate(cert);
        assert!(conn.get_certificate().is_some());
        assert_eq!(conn.get_certificate().unwrap().get_pem(), FAKE_PEM);
    }

    // 4. set_certificate() / get_certificate() round-trip.
    #[test]
    fn test_set_get_certificate() {
        let mut conn = TlsServerConnection::new();
        assert!(conn.get_certificate().is_none());
        conn.set_certificate(make_cert());
        assert!(conn.get_certificate().is_some());
        assert_eq!(conn.get_certificate().unwrap().get_pem(), FAKE_PEM);
    }

    // 5. Authentication mode round-trip through all three variants.
    #[test]
    fn test_authentication_mode() {
        let conn = TlsServerConnection::new();
        assert_eq!(conn.get_authentication_mode(), ClientCertificateMode::Never);

        conn.set_authentication_mode(ClientCertificateMode::Requested);
        assert_eq!(
            conn.get_authentication_mode(),
            ClientCertificateMode::Requested
        );

        conn.set_authentication_mode(ClientCertificateMode::Required);
        assert_eq!(
            conn.get_authentication_mode(),
            ClientCertificateMode::Required
        );

        conn.set_authentication_mode(ClientCertificateMode::Never);
        assert_eq!(conn.get_authentication_mode(), ClientCertificateMode::Never);
    }

    // 6. handshake() succeeds on an open connection and marks done.
    #[test]
    fn test_handshake_success() {
        let conn = TlsServerConnection::new();
        assert!(!conn.is_handshake_done());
        assert!(conn.handshake().is_ok());
        assert!(conn.is_handshake_done());
    }

    // 7. handshake() fails after close().
    #[test]
    fn test_handshake_after_close_fails() {
        let conn = TlsServerConnection::new();
        conn.close();
        assert!(conn.handshake().is_err());
        // handshake_done must remain false — close short-circuits before setting it.
        assert!(!conn.is_handshake_done());
    }

    // 8. Peer certificate and its verification flags are accessible.
    #[test]
    fn test_peer_certificate() {
        let mut conn = TlsServerConnection::new();
        assert!(conn.get_peer_certificate().is_none());

        let mut peer = make_cert();
        peer.set_flags(TlsCertificateFlags::EXPIRED | TlsCertificateFlags::UNKNOWN_CA);
        conn.set_peer_certificate(peer);

        let stored = conn.get_peer_certificate().unwrap();
        assert!(stored.verify().contains(TlsCertificateFlags::EXPIRED));
        assert!(stored.verify().contains(TlsCertificateFlags::UNKNOWN_CA));
        assert!(!stored.is_valid());
    }

    // 9. close() is idempotent and is_closed() reflects it.
    #[test]
    fn test_close_idempotent() {
        let conn = TlsServerConnection::new();
        assert!(!conn.is_closed());
        conn.close();
        assert!(conn.is_closed());
        conn.close(); // second call must not panic
        assert!(conn.is_closed());
    }

    // 10. Full server flow: configure, handshake, receive peer cert, close.
    #[test]
    fn test_full_server_flow() {
        let mut conn = TlsServerConnection::new_with_certificate(make_cert());
        conn.set_authentication_mode(ClientCertificateMode::Required);

        assert!(conn.handshake().is_ok());
        assert!(conn.is_handshake_done());

        conn.set_peer_certificate(make_cert());
        assert!(conn.get_peer_certificate().is_some());

        conn.close();
        assert!(conn.is_closed());
        // Once closed, further handshake attempts must fail.
        assert!(conn.handshake().is_err());
    }
}
