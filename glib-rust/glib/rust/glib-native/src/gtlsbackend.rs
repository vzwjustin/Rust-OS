//! GTlsBackend matching `gio/gtlsbackend.h`.
//!
//! TLS backend interface. In this no_std port we model a simple
//! backend that reports TLS support and tracks a default database.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// A TLS backend (`GTlsBackend`).
pub struct TlsBackend {
    supports_tls: Mutex<bool>,
    supports_dtls: Mutex<bool>,
    default_database: Mutex<Option<String>>,
}

impl TlsBackend {
    /// Creates a new TLS backend (defaults to supporting TLS, not DTLS).
    pub fn new() -> Self {
        Self {
            supports_tls: Mutex::new(true),
            supports_dtls: Mutex::new(false),
            default_database: Mutex::new(None),
        }
    }

    /// Returns whether the backend supports TLS.
    ///
    /// Mirrors `g_tls_backend_supports_tls`.
    pub fn supports_tls(&self) -> bool {
        *self.supports_tls.lock()
    }

    /// Returns whether the backend supports DTLS.
    ///
    /// Mirrors `g_tls_backend_supports_dtls`.
    pub fn supports_dtls(&self) -> bool {
        *self.supports_dtls.lock()
    }

    /// Sets TLS support flag.
    pub fn set_supports_tls(&self, supported: bool) {
        *self.supports_tls.lock() = supported;
    }

    /// Sets DTLS support flag.
    pub fn set_supports_dtls(&self, supported: bool) {
        *self.supports_dtls.lock() = supported;
    }

    /// Gets the default TLS database name.
    ///
    /// Mirrors `g_tls_backend_get_default_database`.
    pub fn get_default_database(&self) -> Option<String> {
        self.default_database.lock().clone()
    }

    /// Sets the default TLS database name.
    ///
    /// Mirrors `g_tls_backend_set_default_database`.
    pub fn set_default_database(&self, name: &str) {
        *self.default_database.lock() = Some(name.to_string());
    }

    /// Returns the certificate type name.
    ///
    /// Mirrors `g_tls_backend_get_certificate_type`.
    pub fn get_certificate_type(&self) -> &'static str {
        "TlsCertificate"
    }

    /// Returns the client connection type name.
    ///
    /// Mirrors `g_tls_backend_get_client_connection_type`.
    pub fn get_client_connection_type(&self) -> &'static str {
        "TlsClientConnection"
    }

    /// Returns the server connection type name.
    ///
    /// Mirrors `g_tls_backend_get_server_connection_type`.
    pub fn get_server_connection_type(&self) -> &'static str {
        "TlsServerConnection"
    }
}

impl Default for TlsBackend {
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
        let backend = TlsBackend::new();
        assert!(backend.supports_tls());
        assert!(!backend.supports_dtls());
    }

    #[test]
    fn test_set_supports_tls() {
        let backend = TlsBackend::new();
        backend.set_supports_tls(false);
        assert!(!backend.supports_tls());
    }

    #[test]
    fn test_set_supports_dtls() {
        let backend = TlsBackend::new();
        backend.set_supports_dtls(true);
        assert!(backend.supports_dtls());
    }

    #[test]
    fn test_default_database() {
        let backend = TlsBackend::new();
        assert!(backend.get_default_database().is_none());
        backend.set_default_database("system");
        assert_eq!(backend.get_default_database().unwrap(), "system");
    }

    #[test]
    fn test_type_names() {
        let backend = TlsBackend::new();
        assert_eq!(backend.get_certificate_type(), "TlsCertificate");
        assert_eq!(backend.get_client_connection_type(), "TlsClientConnection");
        assert_eq!(backend.get_server_connection_type(), "TlsServerConnection");
    }
}
