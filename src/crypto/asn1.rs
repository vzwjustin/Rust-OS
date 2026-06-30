//! Minimal DER/ASN.1 reader, sufficient for parsing X.509 certificates and
//! PKCS#7 SignedData blobs (mirrors the traversal needs of
//! `crypto/asymmetric_keys/x509_cert_parser.c` upstream, without pulling in
//! a general BER/CER parser — only definite-length DER is supported, which
//! is all that X.509/PKCS7 ever use in practice).

use alloc::vec::Vec;

pub const TAG_BOOLEAN: u8 = 0x01;
pub const TAG_INTEGER: u8 = 0x02;
pub const TAG_BIT_STRING: u8 = 0x03;
pub const TAG_OCTET_STRING: u8 = 0x04;
pub const TAG_NULL: u8 = 0x05;
pub const TAG_OID: u8 = 0x06;
pub const TAG_UTF8STRING: u8 = 0x0c;
pub const TAG_SEQUENCE: u8 = 0x30;
pub const TAG_SET: u8 = 0x31;
pub const TAG_PRINTABLESTRING: u8 = 0x13;
pub const TAG_T61STRING: u8 = 0x14;
pub const TAG_IA5STRING: u8 = 0x16;
pub const TAG_UTCTIME: u8 = 0x17;
pub const TAG_GENERALIZEDTIME: u8 = 0x18;
/// Context-specific constructed [0] (used for the X.509 `version` field and
/// PKCS7 `signedAttrs` / `unsignedAttrs` IMPLICIT tags).
pub const TAG_CONTEXT_0: u8 = 0xa0;

#[derive(Debug, Clone, Copy)]
pub struct Tlv<'a> {
    pub tag: u8,
    /// The value bytes only (no tag/length header).
    pub content: &'a [u8],
    /// The full TLV encoding (tag + length + content). Needed when the
    /// caller must re-sign or hash the exact original bytes (e.g. the
    /// tbsCertificate inside a Certificate, or signedAttrs inside PKCS7).
    pub raw: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asn1Error {
    Truncated,
    BadLength,
    UnexpectedTag,
}

/// Parse a single TLV from the front of `data`, returning it plus whatever
/// bytes remain after it.
pub fn parse_tlv(data: &[u8]) -> Result<(Tlv<'_>, &[u8]), Asn1Error> {
    if data.len() < 2 {
        return Err(Asn1Error::Truncated);
    }
    let tag = data[0];
    let mut pos = 1usize;
    let len_byte = data[pos];
    pos += 1;
    let len = if len_byte & 0x80 == 0 {
        len_byte as usize
    } else {
        let num_bytes = (len_byte & 0x7f) as usize;
        if num_bytes == 0 || num_bytes > 4 {
            return Err(Asn1Error::BadLength);
        }
        if data.len() < pos + num_bytes {
            return Err(Asn1Error::Truncated);
        }
        let mut len = 0usize;
        for i in 0..num_bytes {
            len = (len << 8) | data[pos + i] as usize;
        }
        pos += num_bytes;
        len
    };
    if data.len() < pos + len {
        return Err(Asn1Error::Truncated);
    }
    let content = &data[pos..pos + len];
    let raw = &data[0..pos + len];
    let rest = &data[pos + len..];
    Ok((Tlv { tag, content, raw }, rest))
}

/// Expect a TLV with a specific tag and return it.
pub fn expect_tlv(data: &[u8], tag: u8) -> Result<(Tlv<'_>, &[u8]), Asn1Error> {
    let (tlv, rest) = parse_tlv(data)?;
    if tlv.tag != tag {
        return Err(Asn1Error::UnexpectedTag);
    }
    Ok((tlv, rest))
}

/// Parse every top-level TLV inside a constructed value's content (e.g. the
/// body of a SEQUENCE or SET).
pub fn parse_all(content: &[u8]) -> Result<Vec<Tlv<'_>>, Asn1Error> {
    let mut items = Vec::new();
    let mut rest = content;
    while !rest.is_empty() {
        let (tlv, r) = parse_tlv(rest)?;
        items.push(tlv);
        rest = r;
    }
    Ok(items)
}

/// Strip a leading sign/padding zero byte from a DER INTEGER's content, as
/// is conventional when treating it as an unsigned big-endian magnitude
/// (RSA modulus / exponent, serial numbers, etc).
pub fn integer_bytes(content: &[u8]) -> &[u8] {
    if content.len() > 1 && content[0] == 0x00 {
        &content[1..]
    } else {
        content
    }
}

/// Parse a BIT STRING's content, stripping the leading "unused bits" count
/// byte (always 0 for DER-encoded key/signature material, which is always
/// byte-aligned).
pub fn bit_string_bytes(content: &[u8]) -> Result<&[u8], Asn1Error> {
    if content.is_empty() {
        return Err(Asn1Error::Truncated);
    }
    Ok(&content[1..])
}
