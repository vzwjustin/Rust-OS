//! Minimal PKCS#7 `SignedData` parser + signature verification (mirrors a
//! small slice of `crypto/asymmetric_keys/pkcs7_parser.c` +
//! `pkcs7_verify.c` upstream), scoped to what Linux kernel module signing
//! actually uses: a single `SignerInfo`, SHA-256 digest, RSA PKCS#1 v1.5
//! signature, with the certificate supplied separately (out-of-band) from
//! the system trusted keyring rather than walked from an embedded chain.
//!
//! Deliberate gaps vs upstream (documented rather than silently dropped):
//! - Only one `SignerInfo` is read (index 0); multiply-signed blobs are
//!   not supported.
//! - `signerAttrs`/`authenticatedAttributes` are parsed but the
//!   `messageDigest` attribute is not cross-checked against a hash of the
//!   encapsulated content — callers that need that must do it themselves.
//! - No certificate-chain walk: the caller passes the single trusted
//!   `X509Certificate` to verify against directly.
//! - SHA-1 digest algorithm is recognized but rejected (no SHA-1
//!   primitive in `crypto::`); only SHA-256 verifies.

use super::asn1::{self, Asn1Error, TAG_CONTEXT_0, TAG_SEQUENCE, TAG_SET};
use super::rsa::{self, RsaError};
use super::sha256;
use super::x509::X509Certificate;
use alloc::vec::Vec;

/// id-sha256: 2.16.840.1.101.3.4.2.1
const OID_SHA256: [u8; 9] = [0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];

pub struct SignerInfo {
    pub digest_algorithm_is_sha256: bool,
    /// Raw DER bytes (including the `[0]` tag) of authenticatedAttributes,
    /// if present — when present, this is what's actually RSA-signed
    /// (re-tagged as a SET for hashing per RFC 2315 §9.3), not the
    /// encapsulated content directly.
    pub signed_attrs_raw: Option<Vec<u8>>,
    pub signature: Vec<u8>,
}

pub struct Pkcs7SignedData {
    pub content: Vec<u8>,
    pub signer: SignerInfo,
}

#[derive(Debug)]
pub enum Pkcs7Error {
    Asn1(Asn1Error),
    BadStructure,
    UnsupportedDigestAlgorithm,
    NoSignerInfo,
    Rsa(RsaError),
}

impl From<Asn1Error> for Pkcs7Error {
    fn from(e: Asn1Error) -> Self {
        Pkcs7Error::Asn1(e)
    }
}
impl From<RsaError> for Pkcs7Error {
    fn from(e: RsaError) -> Self {
        Pkcs7Error::Rsa(e)
    }
}

/// Parse a DER `ContentInfo` wrapping a `SignedData`:
/// `ContentInfo ::= SEQUENCE { contentType OID, content [0] EXPLICIT SignedData }`
pub fn parse_signed_data(der: &[u8]) -> Result<Pkcs7SignedData, Pkcs7Error> {
    let (content_info, _) = asn1::expect_tlv(der, TAG_SEQUENCE)?;
    let ci_fields = asn1::parse_all(content_info.content)?;
    let wrapped = ci_fields.get(1).ok_or(Pkcs7Error::BadStructure)?;
    if wrapped.tag != TAG_CONTEXT_0 {
        return Err(Pkcs7Error::BadStructure);
    }
    let (signed_data, _) = asn1::expect_tlv(wrapped.content, TAG_SEQUENCE)?;
    let sd_fields = asn1::parse_all(signed_data.content)?;
    // version INTEGER, digestAlgorithms SET, encapContentInfo SEQUENCE,
    // [certificates [0]], [crls [1]], signerInfos SET
    if sd_fields.len() < 4 {
        return Err(Pkcs7Error::BadStructure);
    }
    let encap_content_info = &sd_fields[2];
    let content = extract_econtent(encap_content_info)?;

    let signer_infos = sd_fields
        .iter()
        .rev()
        .find(|t| t.tag == TAG_SET)
        .ok_or(Pkcs7Error::NoSignerInfo)?;
    let signer_list = asn1::parse_all(signer_infos.content)?;
    let signer_tlv = signer_list.first().ok_or(Pkcs7Error::NoSignerInfo)?;
    let signer = parse_signer_info(signer_tlv)?;

    Ok(Pkcs7SignedData { content, signer })
}

/// `EncapsulatedContentInfo ::= SEQUENCE { eContentType OID, eContent [0] EXPLICIT OCTET STRING OPTIONAL }`
fn extract_econtent(encap: &asn1::Tlv<'_>) -> Result<Vec<u8>, Pkcs7Error> {
    let fields = asn1::parse_all(encap.content)?;
    match fields.get(1) {
        Some(wrapped) if wrapped.tag == TAG_CONTEXT_0 => {
            let (octet, _) = asn1::parse_tlv(wrapped.content)?;
            Ok(octet.content.to_vec())
        }
        _ => Ok(Vec::new()), // detached signature: no embedded content
    }
}

/// `SignerInfo ::= SEQUENCE { version, issuerAndSerialNumber, digestAlgorithm,
///    [authenticatedAttributes [0] IMPLICIT SET OF Attribute],
///    digestEncryptionAlgorithm, encryptedDigest OCTET STRING, ... }`
fn parse_signer_info(t: &asn1::Tlv<'_>) -> Result<SignerInfo, Pkcs7Error> {
    let fields = asn1::parse_all(t.content)?;
    if fields.len() < 5 {
        return Err(Pkcs7Error::BadStructure);
    }
    let digest_alg = &fields[2];
    let alg_fields = asn1::parse_all(digest_alg.content)?;
    let oid = alg_fields.first().ok_or(Pkcs7Error::BadStructure)?;
    let digest_algorithm_is_sha256 = oid.content == OID_SHA256;

    let mut idx = 3;
    let mut signed_attrs_raw = None;
    if fields[idx].tag == TAG_CONTEXT_0 {
        signed_attrs_raw = Some(fields[idx].raw.to_vec());
        idx += 1;
    }
    idx += 1; // digestEncryptionAlgorithm
    let enc_digest = fields.get(idx).ok_or(Pkcs7Error::BadStructure)?;
    let signature = enc_digest.content.to_vec();

    Ok(SignerInfo {
        digest_algorithm_is_sha256,
        signed_attrs_raw,
        signature,
    })
}

/// Verify a parsed PKCS7 `SignedData` against a trusted certificate's RSA
/// public key. Hashes either the `signedAttrs` (re-tagged as a SET, per
/// RFC 2315 §9.3) if present, or the encapsulated content directly
/// otherwise, and checks the RSA PKCS#1 v1.5 signature over that digest.
pub fn verify(signed: &Pkcs7SignedData, cert: &X509Certificate) -> Result<(), Pkcs7Error> {
    if !signed.signer.digest_algorithm_is_sha256 {
        return Err(Pkcs7Error::UnsupportedDigestAlgorithm);
    }

    let digest: [u8; 32] = match &signed.signer.signed_attrs_raw {
        Some(raw) => {
            // raw[0] is the IMPLICIT [0] tag; re-tag as SET (0x31) for hashing.
            let mut retagged = raw.clone();
            if !retagged.is_empty() {
                retagged[0] = TAG_SET;
            }
            sha256::sha256(&retagged)
        }
        None => sha256::sha256(&signed.content),
    };

    rsa::verify_sha256(
        &signed.signer.signature,
        &cert.public_key.modulus,
        &cert.public_key.exponent,
        &digest,
    )?;
    Ok(())
}
