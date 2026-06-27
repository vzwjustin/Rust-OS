//! GTlsCertificate matching `gio/gtlscertificate.h`.
//!
//! Represents a TLS certificate (PEM-encoded). In this no_std port we store
//! PEM bytes as `Vec<u8>` and model the verification flags only.

use alloc::string::String;
use alloc::vec::Vec;

/// Certificate verification errors (`GTlsCertificateFlags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TlsCertificateFlags(pub u32);

impl TlsCertificateFlags {
    /// No errors.
    pub const NO_FLAGS: Self = Self(0);
    /// Certificate has an unknown issuer.
    pub const UNKNOWN_CA: Self = Self(0b0000_0001);
    /// Certificate hostname mismatch.
    pub const BAD_IDENTITY: Self = Self(0b0000_0010);
    /// Certificate is not yet valid.
    pub const NOT_ACTIVATED: Self = Self(0b0000_0100);
    /// Certificate has expired.
    pub const EXPIRED: Self = Self(0b0000_1000);
    /// Certificate has been revoked.
    pub const REVOKED: Self = Self(0b0001_0000);
    /// Certificate uses an insecure algorithm.
    pub const INSECURE: Self = Self(0b0010_0000);
    /// Other error.
    pub const GENERIC_ERROR: Self = Self(0b0100_0000);

    /// Returns `true` if `other`'s bits are all set in `self`.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Returns `true` if no error flags are set.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for TlsCertificateFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// A TLS certificate (`GTlsCertificate`).
#[derive(Clone)]
pub struct TlsCertificate {
    pem: Vec<u8>,
    issuer: Option<String>,
    subject: Option<String>,
    /// Pre-set flags returned by `verify()` (used by tests / platform layer).
    flags: TlsCertificateFlags,
}

impl TlsCertificate {
    /// Creates a certificate from raw PEM bytes.
    ///
    /// Mirrors `g_tls_certificate_new_from_pem`.
    pub fn new_from_pem(pem: &[u8]) -> Self {
        Self {
            pem: pem.to_vec(),
            issuer: None,
            subject: None,
            flags: TlsCertificateFlags::NO_FLAGS,
        }
    }

    /// Creates a certificate with subject and issuer metadata.
    pub fn new_with_metadata(pem: &[u8], subject: &str, issuer: &str) -> Self {
        Self {
            pem: pem.to_vec(),
            issuer: Some(issuer.into()),
            subject: Some(subject.into()),
            flags: TlsCertificateFlags::NO_FLAGS,
        }
    }

    /// Returns the PEM data.
    pub fn get_pem(&self) -> &[u8] {
        &self.pem
    }

    /// Returns the subject distinguished name, if known.
    ///
    /// Mirrors `g_tls_certificate_get_subject_name`.
    pub fn get_subject_name(&self) -> Option<&str> {
        self.subject.as_deref()
    }

    /// Returns the issuer distinguished name, if known.
    ///
    /// Mirrors `g_tls_certificate_get_issuer_name`.
    pub fn get_issuer_name(&self) -> Option<&str> {
        self.issuer.as_deref()
    }

    /// Sets the verification flags (platform / test hook).
    pub fn set_flags(&mut self, flags: TlsCertificateFlags) {
        self.flags = flags;
    }

    /// Returns the verification flags.
    ///
    /// Mirrors `g_tls_certificate_verify` (simplified: no real crypto).
    pub fn verify(&self) -> TlsCertificateFlags {
        self.flags
    }

    /// Returns `true` if verification produced no errors.
    pub fn is_valid(&self) -> bool {
        self.flags == TlsCertificateFlags::NO_FLAGS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_PEM: &[u8] = b"-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----\n";

    #[test]
    fn test_new_from_pem() {
        let cert = TlsCertificate::new_from_pem(FAKE_PEM);
        assert_eq!(cert.get_pem(), FAKE_PEM);
        assert!(cert.is_valid());
        assert!(cert.get_subject_name().is_none());
    }

    #[test]
    fn test_metadata() {
        let cert = TlsCertificate::new_with_metadata(FAKE_PEM, "CN=example.com", "CN=CA");
        assert_eq!(cert.get_subject_name(), Some("CN=example.com"));
        assert_eq!(cert.get_issuer_name(), Some("CN=CA"));
    }

    #[test]
    fn test_expired_flag() {
        let mut cert = TlsCertificate::new_from_pem(FAKE_PEM);
        cert.set_flags(TlsCertificateFlags::EXPIRED);
        assert!(!cert.is_valid());
        assert!(cert.verify().contains(TlsCertificateFlags::EXPIRED));
    }

    #[test]
    fn test_multiple_flags() {
        let mut cert = TlsCertificate::new_from_pem(FAKE_PEM);
        cert.set_flags(TlsCertificateFlags::UNKNOWN_CA | TlsCertificateFlags::BAD_IDENTITY);
        assert!(!cert.is_valid());
        assert!(cert.verify().contains(TlsCertificateFlags::UNKNOWN_CA));
        assert!(cert.verify().contains(TlsCertificateFlags::BAD_IDENTITY));
    }

    #[test]
    fn test_clear_flags() {
        let mut cert = TlsCertificate::new_from_pem(FAKE_PEM);
        cert.set_flags(TlsCertificateFlags::EXPIRED);
        cert.set_flags(TlsCertificateFlags::NO_FLAGS);
        assert!(cert.is_valid());
    }

    #[test]
    fn test_pem_content() {
        let cert = TlsCertificate::new_from_pem(b"abc");
        assert_eq!(cert.get_pem(), b"abc");
    }
}
