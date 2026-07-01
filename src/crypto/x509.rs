//! Minimal X.509 certificate parser (RFC 5280 subset), mirroring a small
//! slice of `crypto/asymmetric_keys/x509_cert_parser.c` upstream: enough to
//! pull a certificate's subject/issuer (best-effort, first string found —
//! we do not build a full RDN sequence), validity window, and RSA
//! subjectPublicKeyInfo out of a DER blob, plus the raw bytes needed to
//! verify the certificate's own signature via PKCS7.
//!
//! Deliberately out of scope: ECDSA/Ed25519 keys, extensions parsing
//! (keyUsage/basicConstraints/subjectKeyIdentifier), and full
//! RFC 4514 DN rendering. Only RSA subject keys are supported — anything
//! else yields `X509Error::UnsupportedKeyAlgo`.

use super::asn1::{self, Asn1Error, Tlv, TAG_CONTEXT_0, TAG_SEQUENCE};
use alloc::string::String;
use alloc::vec::Vec;

/// rsaEncryption OID: 1.2.840.113549.1.1.1
const OID_RSA_ENCRYPTION: [u8; 9] = [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];

pub struct RsaPublicKey {
    /// Big-endian modulus, leading zero stripped.
    pub modulus: Vec<u8>,
    /// Big-endian public exponent.
    pub exponent: Vec<u8>,
}

pub struct X509Certificate {
    pub subject: String,
    pub issuer: String,
    /// Validity timestamps rendered as a packed decimal `YYYYMMDDHHMMSS`
    /// (NOT a true Unix epoch — this kernel has no calendar/epoch
    /// conversion helper available to crypto/; the packed form still sorts
    /// and compares correctly, which is sufficient for expiry checks).
    pub not_before: u64,
    pub not_after: u64,
    pub public_key: RsaPublicKey,
    /// Raw DER bytes of the `tbsCertificate` (tag+length+content), i.e.
    /// exactly the bytes the certificate's own signature was computed over.
    pub tbs_certificate: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug)]
pub enum X509Error {
    Asn1(Asn1Error),
    UnexpectedTag,
    MissingField,
    UnsupportedKeyAlgo,
}

impl From<Asn1Error> for X509Error {
    fn from(e: Asn1Error) -> Self {
        X509Error::Asn1(e)
    }
}

/// Parse a DER-encoded X.509 certificate:
/// `Certificate ::= SEQUENCE { tbsCertificate, signatureAlgorithm, signatureValue }`
pub fn parse_certificate(der: &[u8]) -> Result<X509Certificate, X509Error> {
    let (outer, _) = asn1::expect_tlv(der, TAG_SEQUENCE)?;
    let parts = asn1::parse_all(outer.content)?;
    if parts.len() < 3 {
        return Err(X509Error::MissingField);
    }
    let tbs = parts[0];
    let sig_value = parts[2];

    let tbs_fields = asn1::parse_all(tbs.content)?;
    let mut idx = 0usize;
    if tbs_fields.first().map(|t| t.tag) == Some(TAG_CONTEXT_0) {
        idx += 1; // optional [0] EXPLICIT version
    }
    idx += 1; // serialNumber INTEGER
    idx += 1; // signature AlgorithmIdentifier (signature algo used by the issuer, duplicated outside)

    let issuer = field(&tbs_fields, idx)?;
    let issuer_str = format_name(issuer)?;
    idx += 1;

    let validity = field(&tbs_fields, idx)?;
    idx += 1;
    let (not_before, not_after) = parse_validity(validity)?;

    let subject = field(&tbs_fields, idx)?;
    let subject_str = format_name(subject)?;
    idx += 1;

    let spki = field(&tbs_fields, idx)?;
    let public_key = parse_rsa_spki(spki)?;

    let signature = asn1::bit_string_bytes(sig_value.content)?.to_vec();

    Ok(X509Certificate {
        subject: subject_str,
        issuer: issuer_str,
        not_before,
        not_after,
        public_key,
        tbs_certificate: tbs.raw.to_vec(),
        signature,
    })
}

fn field<'a>(fields: &'a [Tlv<'a>], idx: usize) -> Result<&'a Tlv<'a>, X509Error> {
    fields.get(idx).ok_or(X509Error::MissingField)
}

/// `Name ::= RDNSequence`. We don't build a full DN string; we just
/// concatenate every directory-string AttributeValue we find, separated by
/// `/`, which is enough for logging/identification purposes.
fn format_name(name: &Tlv<'_>) -> Result<String, X509Error> {
    let mut out = String::new();
    let rdns = asn1::parse_all(name.content)?; // SEQUENCE OF RelativeDistinguishedName (SET)
    for rdn in rdns {
        let attrs = asn1::parse_all(rdn.content)?; // SET OF AttributeTypeAndValue
        for attr in attrs {
            let av = asn1::parse_all(attr.content)?; // SEQUENCE { type OID, value ANY }
            if av.len() == 2 {
                if let Ok(s) = core::str::from_utf8(av[1].content) {
                    if !out.is_empty() {
                        out.push('/');
                    }
                    out.push_str(s);
                }
            }
        }
    }
    Ok(out)
}

/// `Validity ::= SEQUENCE { notBefore Time, notAfter Time }` where `Time`
/// is UTCTime (`YYMMDDHHMMSSZ`) or GeneralizedTime (`YYYYMMDDHHMMSSZ`).
fn parse_validity(validity: &Tlv<'_>) -> Result<(u64, u64), X509Error> {
    let times = asn1::parse_all(validity.content)?;
    if times.len() != 2 {
        return Err(X509Error::MissingField);
    }
    Ok((parse_time(&times[0])?, parse_time(&times[1])?))
}

fn parse_time(t: &Tlv<'_>) -> Result<u64, X509Error> {
    let s = core::str::from_utf8(t.content).map_err(|_| X509Error::MissingField)?;
    let digits: Vec<u32> = s
        .chars()
        .filter(|c| c.is_ascii_digit())
        .map(|c| c.to_digit(10).unwrap())
        .collect();
    // UTCTime has a 2-digit year (YY -> assume 20YY for YY<50 else 19YY,
    // per RFC 5280); GeneralizedTime has a 4-digit year.
    let (year_digits, rest): (&[u32], &[u32]) = if digits.len() >= 12 && t.tag == asn1::TAG_UTCTIME
    {
        (&digits[0..2], &digits[2..])
    } else if digits.len() >= 14 {
        (&digits[0..4], &digits[4..])
    } else {
        return Err(X509Error::MissingField);
    };
    let mut year = digits_to_u64(year_digits);
    if year_digits.len() == 2 {
        year += if year < 50 { 2000 } else { 1900 };
    }
    let mut packed = year;
    for chunk in rest.chunks(2).take(5) {
        packed = packed * 100 + digits_to_u64(chunk);
    }
    Ok(packed)
}

fn digits_to_u64(digits: &[u32]) -> u64 {
    digits.iter().fold(0u64, |acc, &d| acc * 10 + d as u64)
}

/// `SubjectPublicKeyInfo ::= SEQUENCE { algorithm AlgorithmIdentifier, subjectPublicKey BIT STRING }`
/// For RSA, the BIT STRING content is itself DER:
/// `RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }`
fn parse_rsa_spki(spki: &Tlv<'_>) -> Result<RsaPublicKey, X509Error> {
    let parts = asn1::parse_all(spki.content)?;
    if parts.len() != 2 {
        return Err(X509Error::MissingField);
    }
    let alg_id = asn1::parse_all(parts[0].content)?;
    let oid = alg_id.first().ok_or(X509Error::MissingField)?;
    if oid.content != OID_RSA_ENCRYPTION {
        return Err(X509Error::UnsupportedKeyAlgo);
    }

    let key_bits = asn1::bit_string_bytes(parts[1].content)?;
    let (key_seq, _) = asn1::expect_tlv(key_bits, TAG_SEQUENCE)?;
    let key_parts = asn1::parse_all(key_seq.content)?;
    if key_parts.len() != 2 {
        return Err(X509Error::MissingField);
    }
    Ok(RsaPublicKey {
        modulus: asn1::integer_bytes(key_parts[0].content).to_vec(),
        exponent: asn1::integer_bytes(key_parts[1].content).to_vec(),
    })
}
