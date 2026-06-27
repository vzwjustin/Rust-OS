//! GDummyTlsBackend matching `gio/gdummytlsbackend.h` /
//! `gio/gdummytlsbackend.c`.
//!
//! A TLS backend that reports no TLS/DTLS support. Used when GIO is
//! built without a real crypto provider. Mirrors `GDummyTlsBackend`.
//!
//! Fully `no_std` compatible.

/// A dummy TLS backend (`GDummyTlsBackend`).
///
/// All connection attempts through this backend would fail at runtime
/// because [`supports_tls`](DummyTlsBackend::supports_tls) and
/// [`supports_dtls`](DummyTlsBackend::supports_dtls) return `false`.
pub struct DummyTlsBackend;

impl DummyTlsBackend {
    /// Creates a new dummy TLS backend.
    ///
    /// Mirrors `g_dummy_tls_backend_new`.
    pub fn new() -> Self {
        Self
    }

    /// Returns `false` — dummy backend does not provide TLS.
    ///
    /// Mirrors `g_tls_backend_supports_tls`.
    pub fn supports_tls(&self) -> bool {
        false
    }

    /// Returns `false` — dummy backend does not provide DTLS.
    ///
    /// Mirrors `g_tls_backend_supports_dtls`.
    pub fn supports_dtls(&self) -> bool {
        false
    }

    /// Returns the GObject type name for certificates from this backend.
    ///
    /// Mirrors `g_tls_backend_get_certificate_type`.
    pub fn get_certificate_type(&self) -> &'static str {
        "DummyTlsCertificate"
    }

    /// Returns the GObject type name for client connections.
    ///
    /// Mirrors `g_tls_backend_get_client_connection_type`.
    pub fn get_client_connection_type(&self) -> &'static str {
        "DummyTlsClientConnection"
    }

    /// Returns the GObject type name for server connections.
    ///
    /// Mirrors `g_tls_backend_get_server_connection_type`.
    pub fn get_server_connection_type(&self) -> &'static str {
        "DummyTlsServerConnection"
    }
}

impl Default for DummyTlsBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_reports_no_tls_or_dtls() {
        let backend = DummyTlsBackend::new();
        assert!(!backend.supports_tls());
        assert!(!backend.supports_dtls());
    }

    #[test]
    fn type_names_match_dummy_types() {
        let backend = DummyTlsBackend::new();
        assert_eq!(backend.get_certificate_type(), "DummyTlsCertificate");
        assert_eq!(
            backend.get_client_connection_type(),
            "DummyTlsClientConnection"
        );
        assert_eq!(
            backend.get_server_connection_type(),
            "DummyTlsServerConnection"
        );
    }

    #[test]
    fn default_is_same_as_new() {
        let a = DummyTlsBackend::default();
        let b = DummyTlsBackend::new();
        assert!(!a.supports_tls());
        assert!(!b.supports_dtls());
    }
}
