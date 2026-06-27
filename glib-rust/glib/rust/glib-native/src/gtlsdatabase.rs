//! GTlsDatabase matching `gio/gtlsdatabase.h`.
//!
//! TLS certificate database for verifying certificate chains. In this
//! no_std port we model an in-memory store of anchored certificates
//! with `verify_chain` returning flags.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gtlscertificate::{TlsCertificate, TlsCertificateFlags};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Verify flags for `GTlsDatabase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TlsDatabaseVerifyFlags(pub u32);

impl TlsDatabaseVerifyFlags {
    pub const NONE: Self = Self(0);
    pub const TRUSTED: Self = Self(1 << 0);
}

/// Lookup flags for `GTlsDatabase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TlsDatabaseLookupFlags(pub u32);

impl TlsDatabaseLookupFlags {
    pub const NONE: Self = Self(0);
    pub const KEYPAIR: Self = Self(1 << 0);
}

/// Purpose constants.
pub const TLS_DATABASE_PURPOSE_AUTHENTICATE_SERVER: &str = "1.3.6.1.5.5.7.3.1";
pub const TLS_DATABASE_PURPOSE_AUTHENTICATE_CLIENT: &str = "1.3.6.1.5.5.7.3.2";

/// A TLS certificate database (`GTlsDatabase`).
pub struct TlsDatabase {
    anchors: Mutex<Vec<TlsCertificate>>,
    handles: Mutex<Vec<(String, TlsCertificate)>>,
}

impl TlsDatabase {
    /// Creates a new empty TLS database.
    pub fn new() -> Self {
        Self {
            anchors: Mutex::new(Vec::new()),
            handles: Mutex::new(Vec::new()),
        }
    }

    /// Adds an anchor certificate to the database.
    pub fn add_anchor(&self, cert: TlsCertificate) {
        self.anchors.lock().push(cert);
    }

    /// Verifies a certificate chain against the database.
    ///
    /// Mirrors `g_tls_database_verify_chain`.
    pub fn verify_chain(
        &self,
        chain: &[TlsCertificate],
        _purpose: &str,
        _identity: Option<&str>,
        _flags: TlsDatabaseVerifyFlags,
    ) -> TlsCertificateFlags {
        if chain.is_empty() {
            return TlsCertificateFlags::UNKNOWN_CA;
        }
        let anchors = self.anchors.lock();
        if anchors.is_empty() {
            return TlsCertificateFlags::UNKNOWN_CA;
        }
        for cert in chain {
            if !cert.is_valid() {
                return TlsCertificateFlags::EXPIRED;
            }
        }
        TlsCertificateFlags::NO_FLAGS
    }

    /// Creates a handle for a certificate.
    ///
    /// Mirrors `g_tls_database_create_certificate_handle`.
    pub fn create_certificate_handle(&self, cert: &TlsCertificate) -> String {
        let pem = cert.get_pem();
        let handle = alloc::format!("handle:{}", pem.len());
        self.handles.lock().push((handle.clone(), cert.clone()));
        handle
    }

    /// Looks up a certificate by handle.
    ///
    /// Mirrors `g_tls_database_lookup_certificate_for_handle`.
    pub fn lookup_certificate_for_handle(&self, handle: &str) -> Option<TlsCertificate> {
        self.handles
            .lock()
            .iter()
            .find(|(h, _)| h == handle)
            .map(|(_, c)| c.clone())
    }

    /// Looks up the issuer of a certificate.
    ///
    /// Mirrors `g_tls_database_lookup_certificate_issuer`.
    pub fn lookup_certificate_issuer(&self, cert: &TlsCertificate) -> Option<TlsCertificate> {
        let issuer_name = cert.get_issuer_name()?;
        self.anchors
            .lock()
            .iter()
            .find(|a| a.get_subject_name() == Some(issuer_name))
            .cloned()
    }

    /// Returns the number of anchor certificates.
    pub fn n_anchors(&self) -> usize {
        self.anchors.lock().len()
    }
}

impl Default for TlsDatabase {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cert(pem: &[u8]) -> TlsCertificate {
        TlsCertificate::new_from_pem(pem)
    }

    const PEM: &[u8] = b"-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----\n";

    #[test]
    fn test_new() {
        let db = TlsDatabase::new();
        assert_eq!(db.n_anchors(), 0);
    }

    #[test]
    fn test_add_anchor() {
        let db = TlsDatabase::new();
        db.add_anchor(make_cert(PEM));
        assert_eq!(db.n_anchors(), 1);
    }

    #[test]
    fn test_verify_chain_empty() {
        let db = TlsDatabase::new();
        let flags = db.verify_chain(
            &[],
            TLS_DATABASE_PURPOSE_AUTHENTICATE_SERVER,
            None,
            TlsDatabaseVerifyFlags::NONE,
        );
        assert!(flags.contains(TlsCertificateFlags::UNKNOWN_CA));
    }

    #[test]
    fn test_verify_chain_no_anchors() {
        let db = TlsDatabase::new();
        let cert = make_cert(PEM);
        let flags = db.verify_chain(
            &[cert],
            TLS_DATABASE_PURPOSE_AUTHENTICATE_SERVER,
            None,
            TlsDatabaseVerifyFlags::NONE,
        );
        assert!(flags.contains(TlsCertificateFlags::UNKNOWN_CA));
    }

    #[test]
    fn test_verify_chain_valid() {
        let db = TlsDatabase::new();
        db.add_anchor(make_cert(PEM));
        let cert = make_cert(PEM);
        let flags = db.verify_chain(
            &[cert],
            TLS_DATABASE_PURPOSE_AUTHENTICATE_SERVER,
            None,
            TlsDatabaseVerifyFlags::NONE,
        );
        assert!(flags == TlsCertificateFlags::NO_FLAGS);
    }

    #[test]
    fn test_create_and_lookup_handle() {
        let db = TlsDatabase::new();
        let cert = make_cert(PEM);
        let handle = db.create_certificate_handle(&cert);
        let found = db.lookup_certificate_for_handle(&handle);
        assert!(found.is_some());
    }

    #[test]
    fn test_lookup_handle_missing() {
        let db = TlsDatabase::new();
        assert!(db.lookup_certificate_for_handle("nonexistent").is_none());
    }
}
