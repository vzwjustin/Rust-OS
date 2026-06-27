//! GDtlsServerConnection matching `gio/gdtlsserverconnection.h`.
//!
//! Server-side DTLS (Datagram TLS) connection over UDP. In this no_std port
//! we model the connection state, certificate handling, client authentication
//! mode, and peer certificate tracking.
//!
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::gtlscertificate::{TlsCertificate, TlsCertificateFlags};
use crate::gtlsserverconnection::ClientCertificateMode;
use spin::Mutex;

/// Server-side DTLS connection (`GDtlsServerConnection`).
///
/// Wraps a DTLS server endpoint: holds the server certificate, controls client
/// authentication mode, and tracks the peer certificate received during the
/// handshake.
pub struct DtlsServerConnection {
    /// Server certificate presented to connecting clients.
    certificate: Option<TlsCertificate>,
    /// Policy for requesting client certificates.
    authentication_mode: Mutex<ClientCertificateMode>,
    /// Certificate supplied by the peer during the handshake.
    peer_certificate: Option<TlsCertificate>,
    /// Verification errors on the peer certificate (if any).
    peer_certificate_errors: TlsCertificateFlags,
    /// Whether the DTLS handshake has completed.
    handshake_done: Mutex<bool>,
    /// Whether the connection has been closed.
    closed: Mutex<bool>,
}

impl DtlsServerConnection {
    /// Creates a new server connection with no certificate and `Never`
    /// authentication mode.
    ///
    /// Mirrors `g_dtls_server_connection_new`.
    pub fn new() -> Self {
        Self {
            certificate: None,
            authentication_mode: Mutex::new(ClientCertificateMode::Never),
            peer_certificate: None,
            peer_certificate_errors: TlsCertificateFlags::NO_FLAGS,
            handshake_done: Mutex::new(false),
            closed: Mutex::new(false),
        }
    }

    /// Creates a new server connection pre-loaded with `cert`.
    ///
    /// Mirrors `g_dtls_server_connection_new` with a certificate argument.
    pub fn new_with_certificate(cert: TlsCertificate) -> Self {
        Self {
            certificate: Some(cert),
            authentication_mode: Mutex::new(ClientCertificateMode::Never),
            peer_certificate: None,
            peer_certificate_errors: TlsCertificateFlags::NO_FLAGS,
            handshake_done: Mutex::new(false),
            closed: Mutex::new(false),
        }
    }

    /// Returns the current client-certificate authentication mode.
    ///
    /// Mirrors `g_dtls_server_connection_get_authentication_mode`.
    pub fn get_authentication_mode(&self) -> ClientCertificateMode {
        *self.authentication_mode.lock()
    }

    /// Sets the client-certificate authentication mode.
    ///
    /// Mirrors `g_dtls_server_connection_set_authentication_mode`.
    pub fn set_authentication_mode(&self, mode: ClientCertificateMode) {
        *self.authentication_mode.lock() = mode;
    }

    /// Replaces the server certificate.
    ///
    /// Mirrors `g_dtls_connection_set_certificate`.
    pub fn set_certificate(&mut self, cert: TlsCertificate) {
        self.certificate = Some(cert);
    }

    /// Returns a reference to the server certificate, if set.
    ///
    /// Mirrors `g_dtls_connection_get_certificate`.
    pub fn get_certificate(&self) -> Option<&TlsCertificate> {
        self.certificate.as_ref()
    }

    /// Performs the DTLS handshake.
    ///
    /// Returns `Ok(())` on success, or `Err(())` if the connection is already
    /// closed.  Sets `handshake_done` to `true` on success.
    ///
    /// Mirrors `g_dtls_connection_handshake`.
    pub fn handshake(&self) -> Result<(), ()> {
        if *self.closed.lock() {
            return Err(());
        }
        *self.handshake_done.lock() = true;
        Ok(())
    }

    /// Returns a reference to the peer certificate received during the
    /// handshake, if available.
    ///
    /// Mirrors `g_dtls_connection_get_peer_certificate`.
    pub fn get_peer_certificate(&self) -> Option<&TlsCertificate> {
        self.peer_certificate.as_ref()
    }

    /// Stores the peer certificate (called by the platform layer after a
    /// successful handshake).
    pub fn set_peer_certificate(&mut self, cert: TlsCertificate) {
        self.peer_certificate = Some(cert);
    }

    /// Returns the verification errors on the peer certificate.
    ///
    /// Mirrors `g_dtls_connection_get_peer_certificate_errors`.
    pub fn get_peer_certificate_errors(&self) -> TlsCertificateFlags {
        self.peer_certificate_errors
    }

    /// Returns `true` if the handshake has completed.
    pub fn is_handshake_done(&self) -> bool {
        *self.handshake_done.lock()
    }

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

impl Default for DtlsServerConnection {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtlscertificate::{TlsCertificate, TlsCertificateFlags};

    fn dummy_cert() -> TlsCertificate {
        TlsCertificate::new_from_pem(
            b"-----BEGIN CERTIFICATE-----\nMIIBIjANBgkq\n-----END CERTIFICATE-----\n",
        )
    }

    #[test]
    fn test_new_defaults() {
        let conn = DtlsServerConnection::new();
        assert_eq!(conn.get_authentication_mode(), ClientCertificateMode::Never);
        assert!(conn.get_certificate().is_none());
        assert!(conn.get_peer_certificate().is_none());
        assert_eq!(
            conn.get_peer_certificate_errors(),
            TlsCertificateFlags::NO_FLAGS
        );
        assert!(!conn.is_handshake_done());
        assert!(!conn.is_closed());
    }

    #[test]
    fn test_default_trait() {
        let conn = DtlsServerConnection::default();
        assert_eq!(conn.get_authentication_mode(), ClientCertificateMode::Never);
        assert!(!conn.is_closed());
    }

    #[test]
    fn test_new_with_certificate() {
        let cert = dummy_cert();
        let conn = DtlsServerConnection::new_with_certificate(cert);
        assert!(conn.get_certificate().is_some());
        assert_eq!(conn.get_authentication_mode(), ClientCertificateMode::Never);
    }

    #[test]
    fn test_set_and_get_certificate() {
        let mut conn = DtlsServerConnection::new();
        assert!(conn.get_certificate().is_none());
        conn.set_certificate(dummy_cert());
        assert!(conn.get_certificate().is_some());
    }

    #[test]
    fn test_authentication_mode_round_trip() {
        let conn = DtlsServerConnection::new();

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

    #[test]
    fn test_handshake_succeeds_when_open() {
        let conn = DtlsServerConnection::new();
        assert!(!conn.is_handshake_done());
        assert!(conn.handshake().is_ok());
        assert!(conn.is_handshake_done());
    }

    #[test]
    fn test_handshake_fails_when_closed() {
        let conn = DtlsServerConnection::new();
        conn.close();
        assert!(conn.handshake().is_err());
        assert!(!conn.is_handshake_done());
    }

    #[test]
    fn test_close_and_is_closed() {
        let conn = DtlsServerConnection::new();
        assert!(!conn.is_closed());
        conn.close();
        assert!(conn.is_closed());
        // Idempotent: closing again must not panic.
        conn.close();
        assert!(conn.is_closed());
    }

    #[test]
    fn test_peer_certificate_round_trip() {
        let mut conn = DtlsServerConnection::new();
        assert!(conn.get_peer_certificate().is_none());

        conn.set_peer_certificate(dummy_cert());
        assert!(conn.get_peer_certificate().is_some());
    }

    #[test]
    fn test_peer_certificate_errors_default() {
        let conn = DtlsServerConnection::new();
        assert!(!conn
            .get_peer_certificate_errors()
            .contains(TlsCertificateFlags::EXPIRED));
        assert!(!conn
            .get_peer_certificate_errors()
            .contains(TlsCertificateFlags::UNKNOWN_CA));
        assert_eq!(
            conn.get_peer_certificate_errors(),
            TlsCertificateFlags::NO_FLAGS
        );
    }
}
