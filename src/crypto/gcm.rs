//! AES-CTR and AES-GCM (NIST SP 800-38A / SP 800-38D).
//!
//! Built on the AES block cipher in [`super::aes`]. These are the AEAD and
//! counter-mode primitives QUIC packet protection (RFC 9001) needs:
//!   - AES-CTR keystream for the payload / header-protection mask,
//!   - AES-128/256-GCM (AEAD) for authenticated packet encryption.
//!
//! The GHASH multiply uses the textbook right-shift algorithm over GF(2^128)
//! with the reduction polynomial R = 0xE1‖0^120 (SP 800-38D §6.3). It is
//! constant-with-respect-to-data in structure (no data-dependent branches on
//! the multiply path beyond the public bit pattern of the operands), and tag
//! comparison is constant-time.

use super::aes::{encrypt_block, expand_key, AesContext, BLOCK_SIZE};
use super::algapi::CryptoError;
use alloc::vec::Vec;

/// GCM authentication tag length in bytes.
pub const GCM_TAG_LEN: usize = 16;
/// GCM nonce length in bytes (96-bit IV — the only length QUIC uses).
pub const GCM_NONCE_LEN: usize = 12;

fn valid_key_len(len: usize) -> bool {
    len == 16 || len == 24 || len == 32
}

/// Increment the rightmost 32 bits of a 128-bit counter block (big-endian),
/// wrapping mod 2^32 (SP 800-38D §6.2, inc_32).
fn inc32(counter: &mut [u8; BLOCK_SIZE]) {
    let mut c = u32::from_be_bytes([counter[12], counter[13], counter[14], counter[15]]);
    c = c.wrapping_add(1);
    counter[12..16].copy_from_slice(&c.to_be_bytes());
}

/// GCTR: XOR `data` with the AES-CTR keystream starting at counter `icb`.
fn gctr(ctx: &AesContext, icb: [u8; BLOCK_SIZE], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut cb = icb;
    for chunk in data.chunks(BLOCK_SIZE) {
        let ks = encrypt_block(ctx, &cb);
        for (i, &b) in chunk.iter().enumerate() {
            out.push(b ^ ks[i]);
        }
        inc32(&mut cb);
    }
    out
}

/// Multiply two blocks in GF(2^128) (SP 800-38D §6.3, "Algorithm 1").
///
/// Branchless: the per-bit accumulate and the reduction are selected with
/// bitwise masks rather than `if`, so the running time does not depend on the
/// bits of `x` (ciphertext/AAD-derived) or `h` (the secret hash key). A
/// data-dependent branch here would leak `h` through branch-predictor / cache
/// timing.
fn gf_mul(x: &[u8; BLOCK_SIZE], h: &[u8; BLOCK_SIZE]) -> [u8; BLOCK_SIZE] {
    let mut z = [0u8; BLOCK_SIZE];
    let mut v = *h;
    for i in 0..128 {
        // Accumulate v iff bit i of x (MSB first) is set — masked, not branched.
        let bit = (x[i / 8] >> (7 - (i % 8))) & 1;
        let mask = 0u8.wrapping_sub(bit); // 0x00 or 0xFF
        for j in 0..BLOCK_SIZE {
            z[j] ^= v[j] & mask;
        }
        // v >>= 1 over the 128-bit big-endian value; reduce (XOR R into the
        // high byte) iff the bit shifted out of the low end was set.
        let lsb = v[BLOCK_SIZE - 1] & 1;
        let reduce_mask = 0u8.wrapping_sub(lsb);
        for j in (1..BLOCK_SIZE).rev() {
            v[j] = (v[j] >> 1) | ((v[j - 1] & 1) << 7);
        }
        v[0] = (v[0] >> 1) ^ (0xe1 & reduce_mask);
    }
    z
}

/// Compute GHASH_H over AAD ‖ pad ‖ C ‖ pad ‖ [len(AAD)]64 ‖ [len(C)]64
/// (bit lengths, big-endian) in a streaming fashion.
///
/// GHASH is a block-by-block accumulator, so it is computed directly over the
/// caller's buffers with zero heap allocation and no copying of the inputs —
/// only a 16-byte stack block for the final (possibly partial) chunk of each
/// segment and the length block.
fn compute_ghash(h: &[u8; BLOCK_SIZE], aad: &[u8], ciphertext: &[u8]) -> [u8; BLOCK_SIZE] {
    let mut y = [0u8; BLOCK_SIZE];
    let mut absorb = |segment: &[u8], y: &mut [u8; BLOCK_SIZE]| {
        for chunk in segment.chunks(BLOCK_SIZE) {
            let mut block = [0u8; BLOCK_SIZE];
            block[..chunk.len()].copy_from_slice(chunk);
            for j in 0..BLOCK_SIZE {
                y[j] ^= block[j];
            }
            *y = gf_mul(y, h);
        }
    };
    absorb(aad, &mut y);
    absorb(ciphertext, &mut y);

    let mut len_block = [0u8; BLOCK_SIZE];
    len_block[0..8].copy_from_slice(&((aad.len() as u64) * 8).to_be_bytes());
    len_block[8..16].copy_from_slice(&((ciphertext.len() as u64) * 8).to_be_bytes());
    for j in 0..BLOCK_SIZE {
        y[j] ^= len_block[j];
    }
    gf_mul(&y, h)
}

/// Compute J0 for a 96-bit nonce: IV ‖ 0^31 ‖ 1 (SP 800-38D §7.1).
fn j0_from_nonce(nonce: &[u8]) -> [u8; BLOCK_SIZE] {
    let mut j0 = [0u8; BLOCK_SIZE];
    j0[..GCM_NONCE_LEN].copy_from_slice(nonce);
    j0[BLOCK_SIZE - 1] = 1;
    j0
}

/// Encrypt and authenticate `plaintext` with AES-GCM.
///
/// Returns `ciphertext ‖ tag` (the tag is the trailing [`GCM_TAG_LEN`] bytes).
/// `nonce` must be 12 bytes; `key` 16/24/32 bytes.
pub fn aes_gcm_seal(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if !valid_key_len(key.len()) {
        return Err(CryptoError::InvalidKeySize);
    }
    if nonce.len() != GCM_NONCE_LEN {
        return Err(CryptoError::InvalidIvLength);
    }
    let ctx = expand_key(key);
    let h = encrypt_block(&ctx, &[0u8; BLOCK_SIZE]);
    let j0 = j0_from_nonce(nonce);

    let mut counter = j0;
    inc32(&mut counter);
    let ciphertext = gctr(&ctx, counter, plaintext);

    let s = compute_ghash(&h, aad, &ciphertext);
    let tag = gctr(&ctx, j0, &s); // == s XOR E_K(J0)

    let mut out = ciphertext;
    out.extend_from_slice(&tag[..GCM_TAG_LEN]);
    Ok(out)
}

/// Verify and decrypt an AES-GCM `ciphertext ‖ tag`, returning the plaintext.
///
/// Returns [`CryptoError::AuthenticationFailed`] (without exposing plaintext) if
/// the tag does not match. Comparison is constant-time.
pub fn aes_gcm_open(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    ciphertext_and_tag: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if !valid_key_len(key.len()) {
        return Err(CryptoError::InvalidKeySize);
    }
    if nonce.len() != GCM_NONCE_LEN {
        return Err(CryptoError::InvalidIvLength);
    }
    if ciphertext_and_tag.len() < GCM_TAG_LEN {
        return Err(CryptoError::AuthenticationFailed);
    }
    let split = ciphertext_and_tag.len() - GCM_TAG_LEN;
    let (ciphertext, tag) = ciphertext_and_tag.split_at(split);

    let ctx = expand_key(key);
    let h = encrypt_block(&ctx, &[0u8; BLOCK_SIZE]);
    let j0 = j0_from_nonce(nonce);

    let s = compute_ghash(&h, aad, ciphertext);
    let expected = gctr(&ctx, j0, &s);

    // Constant-time tag comparison. Bind both operands to fixed-size arrays so
    // the loop carries no bounds checks (which would be data-independent here
    // anyway, but this keeps the comparison provably branchless).
    let expected_tag: &[u8; GCM_TAG_LEN] = expected[..GCM_TAG_LEN]
        .try_into()
        .map_err(|_| CryptoError::AuthenticationFailed)?;
    let actual_tag: &[u8; GCM_TAG_LEN] =
        tag.try_into().map_err(|_| CryptoError::AuthenticationFailed)?;
    let mut diff = 0u8;
    for i in 0..GCM_TAG_LEN {
        diff |= expected_tag[i] ^ actual_tag[i];
    }
    if diff != 0 {
        return Err(CryptoError::AuthenticationFailed);
    }

    let mut counter = j0;
    inc32(&mut counter);
    Ok(gctr(&ctx, counter, ciphertext))
}

/// AES-CTR: XOR `data` with the keystream generated from the 16-byte initial
/// counter block `counter`. Symmetric — the same call decrypts.
pub fn aes_ctr_xor(key: &[u8], counter: &[u8], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if !valid_key_len(key.len()) {
        return Err(CryptoError::InvalidKeySize);
    }
    if counter.len() != BLOCK_SIZE {
        return Err(CryptoError::InvalidIvLength);
    }
    let ctx = expand_key(key);
    let mut icb = [0u8; BLOCK_SIZE];
    icb.copy_from_slice(counter);
    Ok(gctr(&ctx, icb, data))
}

/// Encrypt a single block with raw AES (ECB). Used for QUIC header protection,
/// which derives a 5-byte mask from `AES-ECB(hp_key, sample)` (RFC 9001 §5.4.3).
pub fn aes_ecb_encrypt_block(key: &[u8], block: &[u8; BLOCK_SIZE]) -> Result<[u8; BLOCK_SIZE], CryptoError> {
    if !valid_key_len(key.len()) {
        return Err(CryptoError::InvalidKeySize);
    }
    let ctx = expand_key(key);
    Ok(encrypt_block(&ctx, block))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(s: &str) -> Vec<u8> {
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len() / 2);
        let val = |c: u8| match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => 0,
        };
        let mut i = 0;
        while i + 1 < b.len() {
            out.push((val(b[i]) << 4) | val(b[i + 1]));
            i += 2;
        }
        out
    }

    // NIST GCM test case 1: empty plaintext and AAD.
    #[test]
    fn gcm_tc1_empty() {
        let key = [0u8; 16];
        let nonce = [0u8; 12];
        let out = aes_gcm_seal(&key, &nonce, &[], &[]).unwrap();
        assert_eq!(out, h("58e2fccefa7e3061367f1d57a4e7455a"));
        let pt = aes_gcm_open(&key, &nonce, &[], &out).unwrap();
        assert!(pt.is_empty());
    }

    // NIST GCM test case 2: one zero block, no AAD.
    #[test]
    fn gcm_tc2_one_block() {
        let key = [0u8; 16];
        let nonce = [0u8; 12];
        let pt = [0u8; 16];
        let out = aes_gcm_seal(&key, &nonce, &[], &pt).unwrap();
        let expected = h("0388dace60b6a392f328c2b971b2fe78ab6e47d42cec13bdf53a67b21257bddf");
        assert_eq!(out, expected);
        let dec = aes_gcm_open(&key, &nonce, &[], &out).unwrap();
        assert_eq!(dec, pt);
    }

    // NIST/McGrew GCM test case 3: 64-byte plaintext, non-trivial key, no AAD.
    #[test]
    fn gcm_tc3_multiblock() {
        let key = h("feffe9928665731c6d6a8f9467308308");
        let nonce = h("cafebabefacedbaddecaf888");
        let pt = h("d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a72\
                    1c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b391aafd255");
        let out = aes_gcm_seal(&key, &nonce, &[], &pt).unwrap();
        let expected_ct = h("42831ec2217774244b7221b784d0d49ce3aa212f2c02a4e035c17e2329aca12e\
                             21d514b25466931c7d8f6a5aac84aa051ba30b396a0aac973d58e091473f5985");
        let expected_tag = h("4d5c2af327cd64a62cf35abd2ba6fab4");
        assert_eq!(&out[..pt.len()], &expected_ct[..]);
        assert_eq!(&out[pt.len()..], &expected_tag[..]);
        // Round-trip decrypt.
        assert_eq!(aes_gcm_open(&key, &nonce, &[], &out).unwrap(), pt);
    }

    #[test]
    fn gcm_open_rejects_tampered_tag() {
        let key = [0u8; 16];
        let nonce = [0u8; 12];
        let mut out = aes_gcm_seal(&key, &nonce, &[], &[1, 2, 3, 4]).unwrap();
        let last = out.len() - 1;
        out[last] ^= 0x01;
        assert_eq!(
            aes_gcm_open(&key, &nonce, &[], &out),
            Err(CryptoError::AuthenticationFailed)
        );
    }

    #[test]
    fn ctr_is_symmetric() {
        let key = [0x2bu8; 16];
        let ctr = [0x11u8; 16];
        let data = b"the quick brown fox jumps over!!";
        let enc = aes_ctr_xor(&key, &ctr, data).unwrap();
        let dec = aes_ctr_xor(&key, &ctr, &enc).unwrap();
        assert_eq!(dec, data);
    }
}
