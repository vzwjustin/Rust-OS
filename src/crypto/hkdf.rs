//! HMAC-SHA256 (RFC 2104), HKDF (RFC 5869), and the TLS 1.3
//! `HKDF-Expand-Label` construction (RFC 8446 §7.1) used by QUIC key
//! derivation (RFC 9001 §5).
//!
//! Built on the SHA-256 in [`super::sha256`]. These are the key-schedule
//! primitives QUIC needs to derive Initial keys from the connection ID and to
//! expand traffic secrets into AEAD key / IV / header-protection key.

use super::sha256::{sha256, Sha256, BLOCK_SIZE, DIGEST_SIZE};
use alloc::vec::Vec;

/// HMAC-SHA256 (RFC 2104).
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; DIGEST_SIZE] {
    // Normalize the key to one block: hash if too long, then zero-pad.
    let mut key_block = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let digest = sha256(key);
        key_block[..DIGEST_SIZE].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK_SIZE];
    let mut opad = [0x5cu8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }

    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(msg);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(&inner_digest);
    outer.finalize()
}

/// HKDF-Extract (RFC 5869 §2.2): PRK = HMAC(salt, IKM). An empty salt is
/// treated as `DIGEST_SIZE` zero bytes.
pub fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; DIGEST_SIZE] {
    if salt.is_empty() {
        hmac_sha256(&[0u8; DIGEST_SIZE], ikm)
    } else {
        hmac_sha256(salt, ikm)
    }
}

/// HKDF-Expand (RFC 5869 §2.3). `length` must be ≤ 255 * 32.
pub fn hkdf_expand(prk: &[u8], info: &[u8], length: usize) -> Vec<u8> {
    let mut okm = Vec::with_capacity(length);
    let mut t: Vec<u8> = Vec::new();
    let mut counter: u8 = 1;
    while okm.len() < length {
        let mut input = Vec::with_capacity(t.len() + info.len() + 1);
        input.extend_from_slice(&t);
        input.extend_from_slice(info);
        input.push(counter);
        let block = hmac_sha256(prk, &input);
        t = block.to_vec();
        let take = core::cmp::min(length - okm.len(), DIGEST_SIZE);
        okm.extend_from_slice(&block[..take]);
        counter = counter.wrapping_add(1);
    }
    okm
}

/// TLS 1.3 `HKDF-Expand-Label` (RFC 8446 §7.1):
/// `HKDF-Expand(secret, HkdfLabel, length)` where HkdfLabel is
/// `u16(length) ‖ u8(len) ‖ "tls13 "+label ‖ u8(len) ‖ context`.
///
/// `label` is the bare label (e.g. `b"quic key"`); the `"tls13 "` prefix is
/// added here.
pub fn hkdf_expand_label(secret: &[u8], label: &[u8], context: &[u8], length: usize) -> Vec<u8> {
    const PREFIX: &[u8] = b"tls13 ";
    let mut full_label = Vec::with_capacity(PREFIX.len() + label.len());
    full_label.extend_from_slice(PREFIX);
    full_label.extend_from_slice(label);

    let mut info = Vec::with_capacity(2 + 1 + full_label.len() + 1 + context.len());
    info.extend_from_slice(&(length as u16).to_be_bytes());
    info.push(full_label.len() as u8);
    info.extend_from_slice(&full_label);
    info.push(context.len() as u8);
    info.extend_from_slice(context);

    hkdf_expand(secret, &info, length)
}

#[cfg(test)]
mod tests {
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

    // RFC 4231 HMAC-SHA256 test case 2.
    #[test]
    fn hmac_rfc4231_tc2() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            mac.to_vec(),
            h("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
        );
    }

    // RFC 5869 HKDF-SHA256 test case 1.
    #[test]
    fn hkdf_rfc5869_tc1() {
        let ikm = h("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = h("000102030405060708090a0b0c");
        let info = h("f0f1f2f3f4f5f6f7f8f9");
        let prk = hkdf_extract(&salt, &ikm);
        assert_eq!(
            prk.to_vec(),
            h("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5")
        );
        let okm = hkdf_expand(&prk, &info, 42);
        assert_eq!(
            okm,
            h("3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf\
               34007208d5b887185865")
        );
    }
}
