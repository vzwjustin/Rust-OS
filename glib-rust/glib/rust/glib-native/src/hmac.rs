//! HMAC (keyed-hash message authentication code) matching `ghmac.h` / `ghmac.c`.
//!
//! Built on top of [`crate::checksum::Checksum`]. Supports the same algorithms
//! as `GChecksumType`: MD5, SHA-1, SHA-256, SHA-384, SHA-512.

use crate::bytes::Bytes;
use crate::checksum::{checksum_type_get_length, Checksum, ChecksumType};
use crate::prelude::*;

/// Block size in bytes for the hash algorithms.
///
/// MD5 and SHA-1 use 64-byte blocks; SHA-384 and SHA-512 use 128-byte blocks.
fn block_size(checksum_type: ChecksumType) -> usize {
    match checksum_type {
        ChecksumType::Md5 | ChecksumType::Sha1 | ChecksumType::Sha256 => 64,
        ChecksumType::Sha384 | ChecksumType::Sha512 => 128,
    }
}

/// HMAC state for incremental computation (`GHmac`).
#[derive(Clone)]
pub struct Hmac {
    checksum_type: ChecksumType,
    inner: Checksum,
    outer_key: Vec<u8>,
}

impl Hmac {
    /// Create a new HMAC context (`g_hmac_new`).
    pub fn new(checksum_type: ChecksumType, key: &[u8]) -> Self {
        let block = block_size(checksum_type);
        let digest_len = checksum_type_get_length(checksum_type).unwrap_or(0);

        // If key is longer than block size, hash it down to digest length
        let processed_key: Vec<u8> = if key.len() > block {
            let mut cs = Checksum::new(checksum_type);
            cs.update(key);
            let mut digest = vec![0u8; digest_len];
            cs.get_digest(&mut digest);
            digest
        } else {
            key.to_vec()
        };

        // Pad key to block size with zeros
        let mut padded_key = processed_key;
        padded_key.resize(block, 0u8);

        // Inner key: key XOR 0x36
        let inner_key: Vec<u8> = padded_key.iter().map(|b| b ^ 0x36).collect();
        // Outer key: key XOR 0x5c
        let outer_key: Vec<u8> = padded_key.iter().map(|b| b ^ 0x5c).collect();

        let mut inner = Checksum::new(checksum_type);
        inner.update(&inner_key);

        Self {
            checksum_type,
            inner,
            outer_key,
        }
    }

    /// Feed data into the HMAC (`g_hmac_update`).
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// Reset the HMAC to its initial state with the same key.
    pub fn reset(&mut self) {
        self.inner = Checksum::new(self.checksum_type);
        let inner_key: Vec<u8> = self.outer_key.iter().map(|b| b ^ 0x36 ^ 0x5c).collect();
        self.inner.update(&inner_key);
    }

    /// Get the HMAC as a lowercase hexadecimal string (`g_hmac_get_string`).
    pub fn get_string(&self) -> String {
        let digest = self.clone().into_digest();
        hex_encode(&digest)
    }

    /// Get the raw digest bytes into `buffer` (`g_hmac_get_digest`).
    /// Returns the number of bytes written.
    pub fn get_digest(&self, buffer: &mut [u8]) -> usize {
        let digest = self.clone().into_digest();
        let len = digest.len().min(buffer.len());
        buffer[..len].copy_from_slice(&digest[..len]);
        len
    }

    /// Consume the HMAC and produce the final digest bytes.
    fn into_digest(self) -> Vec<u8> {
        let digest_len = checksum_type_get_length(self.checksum_type).unwrap_or(0);

        // Finalize inner hash: H(K ^ 0x36 || data)
        let mut inner_digest = vec![0u8; digest_len];
        self.inner.get_digest(&mut inner_digest);

        // Outer hash: H(K ^ 0x5c || inner_digest)
        let mut outer = Checksum::new(self.checksum_type);
        outer.update(&self.outer_key);
        outer.update(&inner_digest);

        let mut result = vec![0u8; digest_len];
        outer.get_digest(&mut result);
        result
    }
}

/// Compute HMAC of `data` as a hex string (`g_compute_hmac_for_data`).
pub fn compute_hmac_for_data(checksum_type: ChecksumType, key: &[u8], data: &[u8]) -> String {
    let mut hmac = Hmac::new(checksum_type, key);
    hmac.update(data);
    hmac.get_string()
}

/// Compute HMAC of a string as a hex string (`g_compute_hmac_for_string`).
pub fn compute_hmac_for_string(checksum_type: ChecksumType, key: &[u8], s: &str) -> String {
    compute_hmac_for_data(checksum_type, key, s.as_bytes())
}

/// Compute HMAC of `Bytes` as a hex string (`g_compute_hmac_for_bytes`).
pub fn compute_hmac_for_bytes(checksum_type: ChecksumType, key: &Bytes, data: &Bytes) -> String {
    let mut hmac = Hmac::new(checksum_type, key.as_ref());
    hmac.update(data.as_ref());
    hmac.get_string()
}

fn hex_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for &b in data {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 4231 test vector 1: key = 0x0b * 20, data = "Hi There"
    #[test]
    fn hmac_sha256_rfc4231_test1() {
        let key = [0x0bu8; 20];
        let data = b"Hi There";
        let result = compute_hmac_for_data(ChecksumType::Sha256, &key, data);
        assert_eq!(
            result,
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    // RFC 4231 test vector 2: key = "Jefe", data = "what do ya want for nothing?"
    #[test]
    fn hmac_sha256_rfc4231_test2() {
        let result = compute_hmac_for_string(
            ChecksumType::Sha256,
            b"Jefe",
            "what do ya want for nothing?",
        );
        assert_eq!(
            result,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    // RFC 4231 test vector 4: key = 0x01..0x19, data = 0xcd * 50
    #[test]
    fn hmac_sha256_rfc4231_test4() {
        let key: Vec<u8> = (1..=25u8).collect();
        let data = vec![0xcdu8; 50];
        let result = compute_hmac_for_data(ChecksumType::Sha256, &key, &data);
        assert_eq!(
            result,
            "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b"
        );
    }

    // RFC 4231 test vector 7: key = 0x0b * 131 (longer than block), data = "Hi There"
    #[test]
    fn hmac_sha256_rfc4231_test7() {
        // RFC 4231 TC7: key longer than block size, data longer than block size.
        let key = vec![0xaau8; 131];
        let data = b"This is a test using a larger than block-size key and a \
                     larger than block-size data. The key needs to be hashed \
                     before being used by the HMAC algorithm.";
        let result = compute_hmac_for_data(ChecksumType::Sha256, &key, data);
        assert_eq!(
            result,
            "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2"
        );
    }

    // HMAC-SHA1 test with known vector
    #[test]
    fn hmac_sha1_known() {
        let result = compute_hmac_for_string(
            ChecksumType::Sha1,
            b"key",
            "The quick brown fox jumps over the lazy dog",
        );
        assert_eq!(result, "de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9");
    }

    // HMAC-MD5 test with known vector
    #[test]
    fn hmac_md5_known() {
        let result = compute_hmac_for_string(
            ChecksumType::Md5,
            b"key",
            "The quick brown fox jumps over the lazy dog",
        );
        assert_eq!(result, "80070713463e7749b90c2dc24911e275");
    }

    // Incremental update should match one-shot
    #[test]
    fn incremental_matches_oneshot() {
        let key = b"secret";
        let data = b"Hello, World!";

        let oneshot = compute_hmac_for_data(ChecksumType::Sha256, key, data);

        let mut hmac = Hmac::new(ChecksumType::Sha256, key);
        hmac.update(b"Hello, ");
        hmac.update(b"World!");
        assert_eq!(hmac.get_string(), oneshot);
    }

    // Reset should produce same result
    #[test]
    fn reset_works() {
        let key = b"secret";
        let data = b"test data";

        let mut hmac = Hmac::new(ChecksumType::Sha256, key);
        hmac.update(data);
        let first = hmac.get_string();

        hmac.reset();
        hmac.update(data);
        let second = hmac.get_string();

        assert_eq!(first, second);
    }

    // Copy should produce same result
    #[test]
    fn copy_works() {
        let key = b"secret";
        let mut hmac = Hmac::new(ChecksumType::Sha256, key);
        hmac.update(b"test");
        let copied = hmac.clone();
        assert_eq!(hmac.get_string(), copied.get_string());
    }

    // Digest bytes should match hex string
    #[test]
    fn digest_matches_string() {
        let key = b"key";
        let data = b"data";
        let mut hmac = Hmac::new(ChecksumType::Sha256, key);
        hmac.update(data);

        let hex = hmac.get_string();
        let mut buf = vec![0u8; 32];
        let len = hmac.get_digest(&mut buf);
        assert_eq!(len, 32);

        let expected: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex, expected);
    }
}
