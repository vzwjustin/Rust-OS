//! QUIC packet protection (RFC 9001 §5.3–5.4).
//!
//! Combines the AEAD payload protection and the header protection that wrap
//! every QUIC packet, on top of the AES-GCM / AES-CTR primitives in
//! [`crate::crypto`] and the per-level [`PacketKeys`].

use super::keys::PacketKeys;
use crate::crypto::algapi::CryptoError;
use crate::crypto::gcm::{aes_ecb_encrypt_block, aes_gcm_open, aes_gcm_seal};
use alloc::vec::Vec;

/// Length of the header-protection mask consumed from the AES-ECB sample.
pub const HP_MASK_LEN: usize = 5;

/// Construct the AEAD nonce for packet number `pn`: the static IV XORed with
/// the packet number, left-padded into the low bytes (RFC 9001 §5.3).
pub fn packet_nonce(iv: &[u8], pn: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[..12].copy_from_slice(&iv[..12]);
    let pn_bytes = pn.to_be_bytes(); // 8 bytes, big-endian
    for i in 0..8 {
        nonce[12 - 8 + i] ^= pn_bytes[i];
    }
    nonce
}

/// AEAD-protect a packet payload. `aad` is the packet header (with the packet
/// number in the clear); returns `ciphertext ‖ tag` (RFC 9001 §5.3).
pub fn seal(keys: &PacketKeys, pn: u64, aad: &[u8], payload: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if keys.iv.len() < 12 {
        return Err(CryptoError::InvalidIvLength);
    }
    let nonce = packet_nonce(&keys.iv, pn);
    aes_gcm_seal(&keys.key, &nonce, aad, payload)
}

/// Remove AEAD protection, verifying the tag (RFC 9001 §5.3). Returns the
/// recovered payload, or [`CryptoError::AuthenticationFailed`] on a bad tag.
pub fn open(
    keys: &PacketKeys,
    pn: u64,
    aad: &[u8],
    ciphertext_and_tag: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if keys.iv.len() < 12 {
        return Err(CryptoError::InvalidIvLength);
    }
    let nonce = packet_nonce(&keys.iv, pn);
    aes_gcm_open(&keys.key, &nonce, aad, ciphertext_and_tag)
}

/// Compute the 5-byte header-protection mask from a 16-byte ciphertext
/// `sample`, using the AES header-protection key (RFC 9001 §5.4.3).
pub fn header_protection_mask(hp_key: &[u8], sample: &[u8]) -> Result<[u8; HP_MASK_LEN], CryptoError> {
    if sample.len() < 16 {
        return Err(CryptoError::InvalidIvLength);
    }
    let mut block = [0u8; 16];
    block.copy_from_slice(&sample[..16]);
    let enc = aes_ecb_encrypt_block(hp_key, &block)?;
    let mut mask = [0u8; HP_MASK_LEN];
    mask.copy_from_slice(&enc[..HP_MASK_LEN]);
    Ok(mask)
}

/// Apply or remove header protection in place (the operation is its own
/// inverse). `first` is the packet's first byte; `pn_bytes` are the packet
/// number bytes on the wire. `long_header` selects the 4-bit (long) vs 5-bit
/// (short) mask on the first byte (RFC 9001 §5.4.1).
pub fn apply_header_protection(
    mask: &[u8; HP_MASK_LEN],
    first: &mut u8,
    pn_bytes: &mut [u8],
    long_header: bool,
) {
    let low_mask = if long_header { 0x0f } else { 0x1f };
    *first ^= mask[0] & low_mask;
    for (i, b) in pn_bytes.iter_mut().enumerate().take(4) {
        *b ^= mask[1 + i];
    }
}

#[cfg(test)]
mod tests {
    use super::super::keys::initial_keys;
    use super::*;

    fn h(s: &str) -> Vec<u8> {
        let b = s.as_bytes();
        let v = |c: u8| match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => 0,
        };
        let mut o = Vec::new();
        let mut i = 0;
        while i + 1 < b.len() {
            o.push((v(b[i]) << 4) | v(b[i + 1]));
            i += 2;
        }
        o
    }

    #[test]
    fn aead_round_trip() {
        let (client, _server) = initial_keys(&h("8394c8f03e515708"));
        let aad = b"\xc3header-bytes";
        let payload = b"quic crypto frame payload";
        let sealed = seal(&client, 2, aad, payload).unwrap();
        assert_ne!(&sealed[..payload.len()], &payload[..]); // actually encrypted
        let opened = open(&client, 2, aad, &sealed).unwrap();
        assert_eq!(opened, payload);
        // Wrong packet number -> wrong nonce -> auth failure.
        assert!(open(&client, 3, aad, &sealed).is_err());
    }

    // RFC 9001 Appendix A.2: client Initial header-protection sample and mask.
    #[test]
    fn header_protection_rfc9001_a2() {
        let (client, _server) = initial_keys(&h("8394c8f03e515708"));
        let sample = h("d1b1c98dd7689fb8ec11d242b123dc9b");
        let mask = header_protection_mask(&client.hp, &sample).unwrap();
        assert_eq!(mask.to_vec(), h("437b9aec36"));
    }

    #[test]
    fn header_protection_is_involution() {
        let mask = [0xa5u8, 0x11, 0x22, 0x33, 0x44];
        let (mut first, orig_first) = (0xc3u8, 0xc3u8);
        let mut pn = [0x12u8, 0x34, 0x56, 0x78];
        let orig_pn = pn;
        apply_header_protection(&mask, &mut first, &mut pn, true);
        apply_header_protection(&mask, &mut first, &mut pn, true);
        assert_eq!(first, orig_first);
        assert_eq!(pn, orig_pn);
    }
}
