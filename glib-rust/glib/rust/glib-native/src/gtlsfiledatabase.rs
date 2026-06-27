//! GTlsFileDatabase matching `gio/gtlsfiledatabase.h`.
//!
//! A `GTlsDatabase` backed by a file of PEM-encoded anchor certificates.
//! In this no_std port we store the anchors path and delegate to
//! `TlsDatabase` for verification.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gtlscertificate::TlsCertificate;
use crate::gtlsdatabase::TlsDatabase;
use alloc::string::{String, ToString};
use spin::Mutex;

/// A TLS file database (`GTlsFileDatabase`).
pub struct TlsFileDatabase {
    inner: TlsDatabase,
    anchors_path: Mutex<String>,
}

impl TlsFileDatabase {
    /// Creates a new file database from a path to anchor certificates.
    ///
    /// Mirrors `g_tls_file_database_new`.
    pub fn new(anchors: &str) -> Self {
        Self {
            inner: TlsDatabase::new(),
            anchors_path: Mutex::new(anchors.to_string()),
        }
    }

    /// Returns the anchors file path.
    pub fn get_anchors(&self) -> String {
        self.anchors_path.lock().clone()
    }

    /// Sets the anchors file path.
    pub fn set_anchors(&self, path: &str) {
        *self.anchors_path.lock() = path.to_string();
    }

    /// Adds an anchor certificate directly.
    pub fn add_anchor(&self, cert: TlsCertificate) {
        self.inner.add_anchor(cert);
    }

    /// Delegates to `TlsDatabase::verify_chain`.
    pub fn verify_chain(
        &self,
        chain: &[TlsCertificate],
        purpose: &str,
        identity: Option<&str>,
    ) -> crate::gtlscertificate::TlsCertificateFlags {
        use crate::gtlsdatabase::TlsDatabaseVerifyFlags;
        self.inner
            .verify_chain(chain, purpose, identity, TlsDatabaseVerifyFlags::NONE)
    }

    /// Returns the number of anchor certificates.
    pub fn n_anchors(&self) -> usize {
        self.inner.n_anchors()
    }
}

impl Default for TlsFileDatabase {
    fn default() -> Self {
        Self::new("/etc/ssl/certs/ca-certificates.crt")
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtlscertificate::TlsCertificateFlags;

    const PEM: &[u8] = b"-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----\n";

    #[test]
    fn test_new() {
        let db = TlsFileDatabase::new("/custom/path");
        assert_eq!(db.get_anchors(), "/custom/path");
    }

    #[test]
    fn test_default() {
        let db = TlsFileDatabase::default();
        assert!(!db.get_anchors().is_empty());
    }

    #[test]
    fn test_set_anchors() {
        let db = TlsFileDatabase::new("original");
        db.set_anchors("updated");
        assert_eq!(db.get_anchors(), "updated");
    }

    #[test]
    fn test_add_anchor_and_verify() {
        let db = TlsFileDatabase::new("/anchors");
        db.add_anchor(TlsCertificate::new_from_pem(PEM));
        assert_eq!(db.n_anchors(), 1);
        let cert = TlsCertificate::new_from_pem(PEM);
        let flags = db.verify_chain(&[cert], "auth", None);
        assert!(flags == TlsCertificateFlags::NO_FLAGS);
    }

    #[test]
    fn test_verify_empty_chain() {
        let db = TlsFileDatabase::new("/anchors");
        let flags = db.verify_chain(&[], "auth", None);
        assert!(flags.contains(TlsCertificateFlags::UNKNOWN_CA));
    }
}
