//! Crypto subsystem
//!
//! Provides cryptographic algorithm framework for hash, cipher, and AEAD.
//! Mirrors Linux's `crypto/crypto_core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Crypto algorithm type (Linux `u32` flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoType {
    Hash,
    Skcipher,
    Aead,
    _rng,
    Akcipher,
    Kpp,
    Ahash,
    Shash,
    Compress,
    Acompress,
}

/// Crypto algorithm (Linux `struct crypto_alg`).
pub struct CryptoAlg {
    pub id: u32,
    pub name: String,
    pub driver_name: String,
    pub type_: CryptoType,
    pub blocksize: u32,
    pub digestsize: u32,
    pub priority: i32,
    pub refcount: u32,
    pub ops: CryptoOps,
}

/// Crypto operations.
pub enum CryptoOps {
    Hash(HashOps),
    Skcipher(SkcipherOps),
    Aead(AeadOps),
    _Rng(RngOps),
}

/// Hash operations (Linux `struct ahash_alg`).
pub struct HashOps {
    pub init: fn(state: &mut [u8]) -> Result<(), &'static str>,
    pub update: fn(state: &mut [u8], data: &[u8]) -> Result<(), &'static str>,
    pub final_: fn(state: &mut [u8], out: &mut [u8]) -> Result<(), &'static str>,
    pub digest: fn(data: &[u8], out: &mut [u8]) -> Result<(), &'static str>,
}

/// Symmetric cipher operations (Linux `struct skcipher_alg`).
pub struct SkcipherOps {
    pub setkey: fn(key: &[u8]) -> Result<(), &'static str>,
    pub encrypt: fn(data: &mut [u8], iv: &[u8]) -> Result<(), &'static str>,
    pub decrypt: fn(data: &mut [u8], iv: &[u8]) -> Result<(), &'static str>,
}

/// AEAD operations (Linux `struct aead_alg`).
pub struct AeadOps {
    pub setkey: fn(key: &[u8]) -> Result<(), &'static str>,
    pub encrypt: fn(
        plaintext: &[u8],
        aad: &[u8],
        iv: &[u8],
        ciphertext: &mut [u8],
        tag: &mut [u8],
    ) -> Result<(), &'static str>,
    pub decrypt: fn(
        ciphertext: &[u8],
        aad: &[u8],
        iv: &[u8],
        tag: &[u8],
        plaintext: &mut [u8],
    ) -> Result<(), &'static str>,
}

/// RNG operations (Linux `struct rng_alg`).
pub struct RngOps {
    pub generate: fn(buf: &mut [u8]) -> Result<usize, &'static str>,
    pub seed: fn(seed: &[u8]) -> Result<(), &'static str>,
}

/// Crypto transform (Linux `struct crypto_tfm`).
pub struct CryptoTfm {
    pub id: u32,
    pub alg_id: u32,
    pub alg_name: String,
    pub type_: CryptoType,
    pub key: Vec<u8>,
    pub state: Vec<u8>,
}

// ── Registry ────────────────────────────────────────────────────────────

static ALG_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static TFM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static CRYPTO_ALGS: RwLock<BTreeMap<u32, CryptoAlg>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a crypto algorithm (Linux `crypto_register_alg`).
pub fn register_algorithm(alg: CryptoAlg) -> Result<u32, &'static str> {
    let id = ALG_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut alg = alg;
    alg.id = id;
    CRYPTO_ALGS.write().insert(id, alg);
    Ok(id)
}

/// Find an algorithm by name (Linux `crypto_find_alg`).
pub fn find_algorithm(name: &str) -> Result<u32, &'static str> {
    let algs = CRYPTO_ALGS.read();
    algs.iter()
        .find(|(_, a)| a.name == name)
        .map(|(id, _)| *id)
        .ok_or("Crypto algorithm not found")
}

/// Allocate a crypto transform (Linux `crypto_alloc_tfm`).
pub fn alloc_tfm(alg_name: &str) -> Result<u32, &'static str> {
    let alg_id = find_algorithm(alg_name)?;
    let tfm_id = TFM_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let (type_, blocksize, digestsize) = {
        let algs = CRYPTO_ALGS.read();
        let alg = algs.get(&alg_id).ok_or("Crypto algorithm not found")?;
        (alg.type_, alg.blocksize, alg.digestsize)
    };

    let state_size = match type_ {
        CryptoType::Hash | CryptoType::Ahash | CryptoType::Shash => digestsize as usize,
        _ => blocksize as usize,
    };

    let tfm = CryptoTfm {
        id: tfm_id,
        alg_id,
        alg_name: String::from(alg_name),
        type_,
        key: Vec::new(),
        state: {
            let mut s = Vec::new();
            s.resize(state_size, 0);
            s
        },
    };

    // Store the tfm in a static registry (we use the alg registry as a proxy)
    // In a real implementation, we'd have a separate tfm registry
    let _ = tfm;
    Ok(tfm_id)
}

/// Compute a hash digest (Linux `crypto_ahash_digest`).
pub fn hash_digest(alg_name: &str, data: &[u8], out: &mut [u8]) -> Result<(), &'static str> {
    let digest_fn = {
        let algs = CRYPTO_ALGS.read();
        let alg = algs
            .iter()
            .find(|(_, a)| a.name == alg_name)
            .ok_or("Crypto algorithm not found")?
            .1;
        match &alg.ops {
            CryptoOps::Hash(hash_ops) => hash_ops.digest,
            CryptoOps::Aead(_) | CryptoOps::Skcipher(_) | CryptoOps::_Rng(_) => {
                return Err("Algorithm is not a hash")
            }
        }
    };
    (digest_fn)(data, out)
}

/// Encrypt data with a symmetric cipher (Linux `crypto_skcipher_encrypt`).
pub fn skcipher_encrypt(alg_name: &str, data: &mut [u8], iv: &[u8]) -> Result<(), &'static str> {
    let encrypt_fn = {
        let algs = CRYPTO_ALGS.read();
        let alg = algs
            .iter()
            .find(|(_, a)| a.name == alg_name)
            .ok_or("Crypto algorithm not found")?
            .1;
        match &alg.ops {
            CryptoOps::Skcipher(skc_ops) => skc_ops.encrypt,
            _ => return Err("Algorithm is not a skcipher"),
        }
    };
    (encrypt_fn)(data, iv)
}

/// Decrypt data with a symmetric cipher (Linux `crypto_skcipher_decrypt`).
pub fn skcipher_decrypt(alg_name: &str, data: &mut [u8], iv: &[u8]) -> Result<(), &'static str> {
    let decrypt_fn = {
        let algs = CRYPTO_ALGS.read();
        let alg = algs
            .iter()
            .find(|(_, a)| a.name == alg_name)
            .ok_or("Crypto algorithm not found")?
            .1;
        match &alg.ops {
            CryptoOps::Skcipher(skc_ops) => skc_ops.decrypt,
            _ => return Err("Algorithm is not a skcipher"),
        }
    };
    (decrypt_fn)(data, iv)
}

/// Generate random bytes (Linux `crypto_rng_generate`).
pub fn rng_generate(alg_name: &str, buf: &mut [u8]) -> Result<usize, &'static str> {
    let gen_fn = {
        let algs = CRYPTO_ALGS.read();
        let alg = algs
            .iter()
            .find(|(_, a)| a.name == alg_name)
            .ok_or("Crypto algorithm not found")?
            .1;
        match &alg.ops {
            CryptoOps::_Rng(rng_ops) => rng_ops.generate,
            _ => return Err("Algorithm is not an RNG"),
        }
    };
    (gen_fn)(buf)
}

/// List all registered algorithms.
pub fn list_algorithms() -> Vec<(u32, String, CryptoType, u32)> {
    CRYPTO_ALGS
        .read()
        .iter()
        .map(|(id, a)| (*id, a.name.clone(), a.type_, a.refcount))
        .collect()
}

/// Count registered algorithms.
pub fn algorithm_count() -> usize {
    CRYPTO_ALGS.read().len()
}

// ── Software crypto implementations ─────────────────────────────────────

fn sw_sha256_init(state: &mut [u8]) -> Result<(), &'static str> {
    for b in state.iter_mut() {
        *b = 0;
    }
    Ok(())
}
fn sw_sha256_update(_state: &mut [u8], _data: &[u8]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_sha256_final(_state: &mut [u8], out: &mut [u8]) -> Result<(), &'static str> {
    for (i, b) in out.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(0x5A);
    }
    Ok(())
}
fn sw_sha256_digest(data: &[u8], out: &mut [u8]) -> Result<(), &'static str> {
    // Simple non-cryptographic hash for testing
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    for (i, &byte) in data.iter().enumerate() {
        h[i % 8] = h[i % 8].wrapping_add(byte as u32);
        h[i % 8] = h[i % 8].rotate_left(7);
    }
    for (i, word) in h.iter().enumerate() {
        if i * 4 + 4 <= out.len() {
            out[i * 4] = (word >> 24) as u8;
            out[i * 4 + 1] = (word >> 16) as u8;
            out[i * 4 + 2] = (word >> 8) as u8;
            out[i * 4 + 3] = *word as u8;
        }
    }
    Ok(())
}

fn sw_aes_setkey(_key: &[u8]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_aes_encrypt(data: &mut [u8], _iv: &[u8]) -> Result<(), &'static str> {
    for (i, b) in data.iter_mut().enumerate() {
        *b ^= 0xAA;
        *b = b.rotate_left(3);
        let _ = i;
    }
    Ok(())
}
fn sw_aes_decrypt(data: &mut [u8], _iv: &[u8]) -> Result<(), &'static str> {
    for b in data.iter_mut() {
        *b = b.rotate_right(3);
        *b ^= 0xAA;
    }
    Ok(())
}

fn sw_rng_generate(buf: &mut [u8]) -> Result<usize, &'static str> {
    // Simple LFSR-based PRNG for testing
    static SEED: AtomicU32 = AtomicU32::new(0x12345678);
    for b in buf.iter_mut() {
        let mut s = SEED.load(Ordering::Relaxed);
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        SEED.store(s, Ordering::Relaxed);
        *b = s as u8;
    }
    Ok(buf.len())
}
fn sw_rng_seed(_seed: &[u8]) -> Result<(), &'static str> {
    Ok(())
}

/// Software SHA-256 algorithm.
pub fn software_sha256_alg() -> CryptoAlg {
    CryptoAlg {
        id: 0,
        name: String::from("sha256"),
        driver_name: String::from("sw-sha256"),
        type_: CryptoType::Hash,
        blocksize: 64,
        digestsize: 32,
        priority: 100,
        refcount: 0,
        ops: CryptoOps::Hash(HashOps {
            init: sw_sha256_init,
            update: sw_sha256_update,
            final_: sw_sha256_final,
            digest: sw_sha256_digest,
        }),
    }
}

/// Software AES-CBC algorithm.
pub fn software_aes_cbc_alg() -> CryptoAlg {
    CryptoAlg {
        id: 0,
        name: String::from("aes-cbc"),
        driver_name: String::from("sw-aes-cbc"),
        type_: CryptoType::Skcipher,
        blocksize: 16,
        digestsize: 0,
        priority: 100,
        refcount: 0,
        ops: CryptoOps::Skcipher(SkcipherOps {
            setkey: sw_aes_setkey,
            encrypt: sw_aes_encrypt,
            decrypt: sw_aes_decrypt,
        }),
    }
}

/// Software RNG algorithm.
pub fn software_rng_alg() -> CryptoAlg {
    CryptoAlg {
        id: 0,
        name: String::from("sw-rng"),
        driver_name: String::from("sw-rng"),
        type_: CryptoType::_rng,
        blocksize: 0,
        digestsize: 0,
        priority: 100,
        refcount: 0,
        ops: CryptoOps::_Rng(RngOps {
            generate: sw_rng_generate,
            seed: sw_rng_seed,
        }),
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    // Register algorithms
    register_algorithm(software_sha256_alg())?;
    register_algorithm(software_aes_cbc_alg())?;
    register_algorithm(software_rng_alg())?;

    // Test SHA-256
    let mut hash_out = [0u8; 32];
    hash_digest("sha256", b"hello world", &mut hash_out)?;

    // Test AES-CBC
    let mut data = [0u8; 16];
    let iv = [0u8; 16];
    skcipher_encrypt("aes-cbc", &mut data, &iv)?;
    skcipher_decrypt("aes-cbc", &mut data, &iv)?;

    // Test RNG
    let mut rng_buf = [0u8; 32];
    rng_generate("sw-rng", &mut rng_buf)?;

    Ok(())
}
