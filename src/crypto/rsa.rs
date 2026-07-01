//! Minimal RSA PKCS#1 v1.5 *signature verification* (RFC 8017 §8.2.2),
//! built on the schoolbook [`bignum`](super::bignum) modexp. This is the
//! "RSA primitive" referenced by `src/crypto/pkcs7.rs` — it intentionally
//! implements only the verify path (public-exponent modexp + padding
//! check), not signing/encryption, since that's all module-signature and
//! X.509 chain verification need.

use super::bignum::BigUint;
use alloc::vec::Vec;

/// DER-encoded `DigestInfo` algorithm prefixes (RFC 8017 Appendix B / RFC
/// 3447 with SHA-256). Only SHA-256 is wired into `crypto::sha256`; SHA-1's
/// prefix is included for parsing legacy certs but rejected in practice
/// since we have no SHA-1 implementation to compute the comparand.
const SHA256_DIGESTINFO_PREFIX: [u8; 19] = [
    0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05,
    0x00, 0x04, 0x20,
];

#[derive(Debug, PartialEq, Eq)]
pub enum RsaError {
    BadSignatureLength,
    BadPadding,
    DigestMismatch,
}

/// Verify a PKCS#1 v1.5 RSA signature over a SHA-256 digest.
///
/// `signature` — raw big-endian signature bytes (same length as the modulus).
/// `modulus`, `exponent` — raw big-endian RSA public key components (as
/// extracted from an X.509 SubjectPublicKeyInfo by `crypto::x509`).
/// `digest` — the 32-byte SHA-256 digest of the signed content.
pub fn verify_sha256(
    signature: &[u8],
    modulus: &[u8],
    exponent: &[u8],
    digest: &[u8; 32],
) -> Result<(), RsaError> {
    let n = BigUint::from_be_bytes(modulus);
    let e = BigUint::from_be_bytes(exponent);
    let s = BigUint::from_be_bytes(signature);

    let k = modulus.len();
    if signature.len() != k || signature.is_empty() {
        return Err(RsaError::BadSignatureLength);
    }

    let m = s.modpow(&e, &n);
    let em = m
        .to_be_bytes_padded(k)
        .ok_or(RsaError::BadSignatureLength)?;

    check_pkcs1v15_padding(&em, digest)
}

/// EM = 0x00 || 0x01 || PS (0xff bytes) || 0x00 || DigestInfo(digest)
fn check_pkcs1v15_padding(em: &[u8], digest: &[u8; 32]) -> Result<(), RsaError> {
    let mut expected: Vec<u8> = Vec::with_capacity(SHA256_DIGESTINFO_PREFIX.len() + 32);
    expected.extend_from_slice(&SHA256_DIGESTINFO_PREFIX);
    expected.extend_from_slice(digest);

    if em.len() < 3 + expected.len() {
        return Err(RsaError::BadPadding);
    }
    if em[0] != 0x00 || em[1] != 0x01 {
        return Err(RsaError::BadPadding);
    }
    let ps_end = em.len() - expected.len();
    // PS must be all 0xff, terminated by a single 0x00 byte just before DigestInfo.
    if em[ps_end - 1] != 0x00 {
        return Err(RsaError::BadPadding);
    }
    if em[2..ps_end - 1].iter().any(|&b| b != 0xff) {
        return Err(RsaError::BadPadding);
    }
    if ps_end < 2 + 8 {
        // RFC 8017 requires PS to be at least 8 bytes.
        return Err(RsaError::BadPadding);
    }
    if &em[ps_end..] != expected.as_slice() {
        return Err(RsaError::DigestMismatch);
    }
    Ok(())
}
