//! Kernel cryptographic subsystem (Linux crypto/algapi style).
//!
//! Provides a central algorithm registry and built-in SHA-256 / AES-CBC
//! implementations. Subsystems register algorithms at boot via [`init`].

pub mod aes;
pub mod algapi;
pub mod gcm;
pub mod hash;
pub mod hkdf;
pub mod sha256;

pub use algapi::{
    crypto_alg_count, crypto_lookup_alg, crypto_register_alg, AlgBase, AlgoType, CipherAlg,
    CryptoAlg, CryptoError, HashAlg,
};

use core::sync::atomic::{AtomicBool, Ordering};

static CRYPTO_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Register built-in kernel-native algorithms.
pub fn init() {
    if CRYPTO_INITIALIZED.swap(true, Ordering::AcqRel) {
        return;
    }

    let _ = crypto_register_alg(CryptoAlg::Hash(HashAlg {
        base: AlgBase {
            name: "sha256",
            driver_name: "sha256-generic",
            algo_type: AlgoType::Hash,
            priority: 100,
        },
        digest_size: sha256::DIGEST_SIZE,
        hash_one_shot: sha256::sha256_vec,
    }));

    let _ = crypto_register_alg(CryptoAlg::Cipher(CipherAlg {
        base: AlgBase {
            name: "aes",
            driver_name: "aes-generic",
            algo_type: AlgoType::Cipher,
            priority: 100,
        },
        block_size: aes::BLOCK_SIZE,
        key_min_size: 16,
        key_max_size: 32,
        encrypt_cbc: aes::cbc_encrypt,
        decrypt_cbc: aes::cbc_decrypt,
    }));

    let _ = crypto_register_alg(CryptoAlg::Cipher(CipherAlg {
        base: AlgBase {
            name: "cbc(aes)",
            driver_name: "cbc-aes-generic",
            algo_type: AlgoType::Cipher,
            priority: 100,
        },
        block_size: aes::BLOCK_SIZE,
        key_min_size: 16,
        key_max_size: 32,
        encrypt_cbc: aes::cbc_encrypt,
        decrypt_cbc: aes::cbc_decrypt,
    }));

    let _ = crypto_register_alg(CryptoAlg::Cipher(CipherAlg {
        base: AlgBase {
            name: "aes128",
            driver_name: "aes128-cbc-generic",
            algo_type: AlgoType::Cipher,
            priority: 100,
        },
        block_size: aes::BLOCK_SIZE,
        key_min_size: 16,
        key_max_size: 16,
        encrypt_cbc: aes::aes128_cbc_encrypt,
        decrypt_cbc: aes::aes128_cbc_decrypt,
    }));

    let _ = crypto_register_alg(CryptoAlg::Cipher(CipherAlg {
        base: AlgBase {
            name: "aes256",
            driver_name: "aes256-cbc-generic",
            algo_type: AlgoType::Cipher,
            priority: 100,
        },
        block_size: aes::BLOCK_SIZE,
        key_min_size: 32,
        key_max_size: 32,
        encrypt_cbc: aes::aes256_cbc_encrypt,
        decrypt_cbc: aes::aes256_cbc_decrypt,
    }));
}

/// Whether the crypto subsystem has completed initialization.
pub fn is_initialized() -> bool {
    CRYPTO_INITIALIZED.load(Ordering::Acquire)
}

/// Encrypt with a registered cipher by algorithm name.
pub fn cipher_encrypt_cbc(
    name: &str,
    key: &[u8],
    iv: &[u8],
    plaintext: &[u8],
) -> Result<alloc::vec::Vec<u8>, CryptoError> {
    let alg = crypto_lookup_alg(name).ok_or(CryptoError::NotFound)?;
    match alg {
        CryptoAlg::Cipher(c) => (c.encrypt_cbc)(key, iv, plaintext),
        CryptoAlg::Hash(_) => Err(CryptoError::NotFound),
    }
}

/// Decrypt with a registered cipher by algorithm name.
pub fn cipher_decrypt_cbc(
    name: &str,
    key: &[u8],
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<alloc::vec::Vec<u8>, CryptoError> {
    let alg = crypto_lookup_alg(name).ok_or(CryptoError::NotFound)?;
    match alg {
        CryptoAlg::Cipher(c) => (c.decrypt_cbc)(key, iv, ciphertext),
        CryptoAlg::Hash(_) => Err(CryptoError::NotFound),
    }
}
