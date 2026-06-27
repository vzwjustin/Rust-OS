//! GDtlsClientConnection matching `gio/gdtlsclientconnection.h`.
//!
//! DTLS (Datagram TLS, i.e. TLS over UDP) client-side connection.  Extends
//! the base `GDtlsConnection` with client-specific state: the server identity
//! used for certificate validation, a list of trusted CA certificates, an
//! optional SSL-3 fallback flag, and a validation-flags override.
//!
//! Actual cryptographic I/O is left to the platform layer; this module only
//! models the GObject property state described in the GIO headers.
//!
//! Fully `no_std` compatible — uses `alloc` and `spin`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

use crate::gtlscertificate::{TlsCertificate, TlsCertificateFlags};

// ─────────────────────────────────────────────────────────────────────────────
// GDtlsClientConnection
// ─────────────────────────────────────────────────────────────────────────────

/// A DTLS client connection (`GDtlsClientConnection`).
///
/// Owns the local certificate and tracks peer-certificate state, a list of
/// accepted certificate authorities, the target server identity, and
/// connection lifecycle flags.
pub struct DtlsClientConnection {
    /// Local certificate presented during handshake.
    certificate: Option<TlsCertificate>,

    /// DER-encoded CA certificates accepted in addition to system trust store.
    ///
    /// Each entry is a raw DER blob; mirrors the `GList *` returned by
    /// `g_dtls_client_connection_get_accepted_cas`.
    accepted_cas: Vec<Vec<u8>>,

    /// Expected server identity used for certificate validation.
    ///
    /// Mirrors the `server-identity` property / `GSocketConnectable`.
    server_identity: Option<String>,

    /// Whether to fall back to SSL 3.0 (deprecated; kept for API parity).
    use_ssl3: bool,

    /// Certificate-verification flags override for this connection.
    ///
    /// Mirrors `GTlsCertificateFlags` on the `validation-flags` property.
    validation_flags: TlsCertificateFlags,

    /// Certificate the server presented during the handshake.
    peer_certificate: Option<TlsCertificate>,

    /// Errors found while verifying the peer certificate.
    peer_certificate_errors: TlsCertificateFlags,

    /// Set to `true` once the DTLS handshake has completed successfully.
    handshake_done: Mutex<bool>,

    /// Set to `true` after `close()` is called.
    closed: Mutex<bool>,
}

impl DtlsClientConnection {
    /// Creates a new `DtlsClientConnection` with default settings.
    ///
    /// Mirrors `g_dtls_client_connection_new` (no underlying socket is
    /// required in this no_std port).
    pub fn new() -> Self {
        Self {
            certificate: None,
            accepted_cas: Vec::new(),
            server_identity: None,
            use_ssl3: false,
            validation_flags: TlsCertificateFlags::NO_FLAGS,
            peer_certificate: None,
            peer_certificate_errors: TlsCertificateFlags::NO_FLAGS,
            handshake_done: Mutex::new(false),
            closed: Mutex::new(false),
        }
    }

    // ── Server identity ───────────────────────────────────────────────────────

    /// Returns the expected server identity, if set.
    ///
    /// Mirrors `g_dtls_client_connection_get_server_identity`.
    pub fn get_server_identity(&self) -> Option<&str> {
        self.server_identity.as_deref()
    }

    /// Sets the expected server identity used for certificate validation.
    ///
    /// Mirrors `g_dtls_client_connection_set_server_identity`.
    pub fn set_server_identity(&mut self, identity: &str) {
        self.server_identity = Some(identity.to_string());
    }

    // ── Validation flags ──────────────────────────────────────────────────────

    /// Returns the certificate-validation flags for this connection.
    ///
    /// Mirrors `g_dtls_client_connection_get_validation_flags`.
    pub fn get_validation_flags(&self) -> TlsCertificateFlags {
        self.validation_flags
    }

    /// Sets the certificate-validation flags for this connection.
    ///
    /// Mirrors `g_dtls_client_connection_set_validation_flags`.
    pub fn set_validation_flags(&mut self, flags: TlsCertificateFlags) {
        self.validation_flags = flags;
    }

    // ── Accepted CAs ──────────────────────────────────────────────────────────

    /// Returns the list of accepted CA certificates (DER blobs).
    ///
    /// Mirrors `g_dtls_client_connection_get_accepted_cas`.
    pub fn get_accepted_cas(&self) -> &[Vec<u8>] {
        &self.accepted_cas
    }

    /// Appends a DER-encoded CA certificate to the accepted-CAs list.
    ///
    /// There is no GIO counterpart; the list is populated by the TLS backend
    /// after the handshake.  Exposed here as a test / platform hook.
    pub fn add_accepted_ca(&mut self, ca: Vec<u8>) {
        self.accepted_cas.push(ca);
    }

    // ── Local certificate ─────────────────────────────────────────────────────

    /// Sets the local certificate presented during the handshake.
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

    // ── Handshake ─────────────────────────────────────────────────────────────

    /// Performs the DTLS handshake.
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

    /// Returns `true` if the DTLS handshake has completed successfully.
    pub fn is_handshake_done(&self) -> bool {
        *self.handshake_done.lock()
    }

    // ── Peer certificate ──────────────────────────────────────────────────────

    /// Returns the peer's certificate as presented during the handshake.
    ///
    /// Mirrors `g_dtls_connection_get_peer_certificate`.
    pub fn get_peer_certificate(&self) -> Option<&TlsCertificate> {
        self.peer_certificate.as_ref()
    }

    /// Returns the verification errors for the peer's certificate.
    ///
    /// Mirrors `g_dtls_connection_get_peer_certificate_errors`.
    pub fn get_peer_certificate_errors(&self) -> TlsCertificateFlags {
        self.peer_certificate_errors
    }

    /// Sets the peer certificate and associated errors (platform / test hook).
    pub fn set_peer_certificate(&mut self, cert: TlsCertificate, errors: TlsCertificateFlags) {
        self.peer_certificate = Some(cert);
        self.peer_certificate_errors = errors;
    }

    // ── SSL 3.0 fallback ──────────────────────────────────────────────────────

    /// Returns whether SSL 3.0 fallback is enabled (deprecated).
    ///
    /// Mirrors the `use-ssl3` property.
    pub fn get_use_ssl3(&self) -> bool {
        self.use_ssl3
    }

    /// Enables or disables SSL 3.0 fallback (deprecated).
    pub fn set_use_ssl3(&mut self, use_ssl3: bool) {
        self.use_ssl3 = use_ssl3;
    }

    // ── Close ─────────────────────────────────────────────────────────────────

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

impl Default for DtlsClientConnection {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_PEM: &[u8] = b"-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n";

    fn make_cert() -> TlsCertificate {
        TlsCertificate::new_from_pem(FAKE_PEM)
    }

    // ── Construction ──────────────────────────────────────────────────────────

    #[test]
    fn test_new_defaults() {
        let conn = DtlsClientConnection::new();
        assert!(conn.get_server_identity().is_none());
        assert!(conn.get_certificate().is_none());
        assert!(conn.get_peer_certificate().is_none());
        assert_eq!(conn.get_accepted_cas().len(), 0);
        assert_eq!(conn.get_validation_flags(), TlsCertificateFlags::NO_FLAGS);
        assert_eq!(
            conn.get_peer_certificate_errors(),
            TlsCertificateFlags::NO_FLAGS
        );
        assert!(!conn.get_use_ssl3());
        assert!(!conn.is_handshake_done());
        assert!(!conn.is_closed());
    }

    #[test]
    fn test_default_equals_new() {
        // Default::default() must be equivalent to ::new()
        let a = DtlsClientConnection::new();
        let b = DtlsClientConnection::default();
        assert_eq!(a.get_server_identity(), b.get_server_identity());
        assert_eq!(a.get_validation_flags(), b.get_validation_flags());
        assert_eq!(a.is_closed(), b.is_closed());
    }

    // ── Server identity ───────────────────────────────────────────────────────

    #[test]
    fn test_set_get_server_identity() {
        let mut conn = DtlsClientConnection::new();
        assert!(conn.get_server_identity().is_none());
        conn.set_server_identity("dtls.example.com");
        assert_eq!(conn.get_server_identity(), Some("dtls.example.com"));
        // Overwrite
        conn.set_server_identity("other.example.com");
        assert_eq!(conn.get_server_identity(), Some("other.example.com"));
    }

    // ── Validation flags ──────────────────────────────────────────────────────

    #[test]
    fn test_set_get_validation_flags() {
        let mut conn = DtlsClientConnection::new();
        let flags = TlsCertificateFlags::EXPIRED | TlsCertificateFlags::BAD_IDENTITY;
        conn.set_validation_flags(flags);
        let got = conn.get_validation_flags();
        assert!(got.contains(TlsCertificateFlags::EXPIRED));
        assert!(got.contains(TlsCertificateFlags::BAD_IDENTITY));
        assert!(!got.contains(TlsCertificateFlags::REVOKED));
    }

    // ── Accepted CAs ──────────────────────────────────────────────────────────

    #[test]
    fn test_add_and_get_accepted_cas() {
        let mut conn = DtlsClientConnection::new();
        let ca1 = vec![0x30u8, 0x82, 0x01, 0x00]; // fake DER prefix
        let ca2 = vec![0x30u8, 0x82, 0x02, 0x00];
        conn.add_accepted_ca(ca1.clone());
        conn.add_accepted_ca(ca2.clone());
        let cas = conn.get_accepted_cas();
        assert_eq!(cas.len(), 2);
        assert_eq!(cas[0], ca1);
        assert_eq!(cas[1], ca2);
    }

    // ── Local certificate ─────────────────────────────────────────────────────

    #[test]
    fn test_set_get_certificate() {
        let mut conn = DtlsClientConnection::new();
        conn.set_certificate(make_cert());
        let cert = conn.get_certificate().expect("certificate should be set");
        assert_eq!(cert.get_pem(), FAKE_PEM);
    }

    // ── Peer certificate ──────────────────────────────────────────────────────

    #[test]
    fn test_set_get_peer_certificate_and_errors() {
        let mut conn = DtlsClientConnection::new();
        let errors = TlsCertificateFlags::UNKNOWN_CA | TlsCertificateFlags::INSECURE;
        conn.set_peer_certificate(make_cert(), errors);
        let peer = conn
            .get_peer_certificate()
            .expect("peer cert should be set");
        assert_eq!(peer.get_pem(), FAKE_PEM);
        let errs = conn.get_peer_certificate_errors();
        assert!(errs.contains(TlsCertificateFlags::UNKNOWN_CA));
        assert!(errs.contains(TlsCertificateFlags::INSECURE));
        assert!(!errs.contains(TlsCertificateFlags::EXPIRED));
    }

    // ── Handshake ─────────────────────────────────────────────────────────────

    #[test]
    fn test_handshake_success() {
        let conn = DtlsClientConnection::new();
        assert!(!conn.is_handshake_done());
        assert!(conn.handshake().is_ok());
        assert!(conn.is_handshake_done());
    }

    #[test]
    fn test_handshake_fails_when_closed() {
        let conn = DtlsClientConnection::new();
        conn.close();
        assert!(conn.handshake().is_err());
        // handshake_done must remain false
        assert!(!conn.is_handshake_done());
    }

    // ── Close ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_close_and_is_closed() {
        let conn = DtlsClientConnection::new();
        assert!(!conn.is_closed());
        conn.close();
        assert!(conn.is_closed());
        // Idempotent
        conn.close();
        assert!(conn.is_closed());
    }

    // ── SSL 3 fallback ────────────────────────────────────────────────────────

    #[test]
    fn test_use_ssl3_flag() {
        let mut conn = DtlsClientConnection::new();
        assert!(!conn.get_use_ssl3());
        conn.set_use_ssl3(true);
        assert!(conn.get_use_ssl3());
        conn.set_use_ssl3(false);
        assert!(!conn.get_use_ssl3());
    }
}
