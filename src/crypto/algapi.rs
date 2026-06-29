//! Linux-style crypto algorithm registry (crypto/algapi).
//!
//! Algorithms register by name and driver name; callers look up by either.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Algorithm category matching Linux `CRYPTO_ALG_TYPE_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlgoType {
    Hash,
    Cipher,
}

/// Base metadata shared by all registered algorithms.
#[derive(Debug, Clone, Copy)]
pub struct AlgBase {
    pub name: &'static str,
    pub driver_name: &'static str,
    pub algo_type: AlgoType,
    pub priority: i32,
}

/// Hash algorithm vtable.
#[derive(Debug, Clone, Copy)]
pub struct HashAlg {
    pub base: AlgBase,
    pub digest_size: usize,
    pub hash_one_shot: fn(&[u8]) -> Vec<u8>,
}

/// Block cipher algorithm vtable (CBC mode).
#[derive(Debug, Clone, Copy)]
pub struct CipherAlg {
    pub base: AlgBase,
    pub block_size: usize,
    pub key_min_size: usize,
    pub key_max_size: usize,
    pub encrypt_cbc: fn(&[u8], &[u8], &[u8]) -> Result<Vec<u8>, CryptoError>,
    pub decrypt_cbc: fn(&[u8], &[u8], &[u8]) -> Result<Vec<u8>, CryptoError>,
}

/// Registered algorithm entry.
#[derive(Debug, Clone, Copy)]
pub enum CryptoAlg {
    Hash(HashAlg),
    Cipher(CipherAlg),
}

/// Registry errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoError {
    AlreadyRegistered,
    NotFound,
    InvalidKeySize,
    InvalidBlockAlignment,
    InvalidIvLength,
}

impl CryptoError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyRegistered => "algorithm already registered",
            Self::NotFound => "algorithm not found",
            Self::InvalidKeySize => "invalid key size",
            Self::InvalidBlockAlignment => "data length not block-aligned",
            Self::InvalidIvLength => "invalid IV length",
        }
    }
}

struct Registry {
    by_name: BTreeMap<String, CryptoAlg>,
    by_driver: BTreeMap<String, CryptoAlg>,
}

impl Registry {
    const fn new() -> Self {
        Self {
            by_name: BTreeMap::new(),
            by_driver: BTreeMap::new(),
        }
    }
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry::new());

fn key(base: &AlgBase) -> String {
    base.name.to_string()
}

fn driver_key(base: &AlgBase) -> String {
    base.driver_name.to_string()
}

/// Register a hash or cipher algorithm.
pub fn crypto_register_alg(alg: CryptoAlg) -> Result<(), CryptoError> {
    let base = match alg {
        CryptoAlg::Hash(ref h) => &h.base,
        CryptoAlg::Cipher(ref c) => &c.base,
    };

    let mut reg = REGISTRY.lock();
    let name = key(base);
    let driver = driver_key(base);

    if reg.by_name.contains_key(&name) || reg.by_driver.contains_key(&driver) {
        return Err(CryptoError::AlreadyRegistered);
    }

    reg.by_name.insert(name, alg);
    reg.by_driver.insert(driver, alg);
    Ok(())
}

/// Unregister by logical algorithm name (e.g. `"sha256"`).
pub fn crypto_unregister_alg(name: &str) -> Result<(), CryptoError> {
    let mut reg = REGISTRY.lock();
    let alg = reg.by_name.remove(name).ok_or(CryptoError::NotFound)?;
    let base = match alg {
        CryptoAlg::Hash(ref h) => &h.base,
        CryptoAlg::Cipher(ref c) => &c.base,
    };
    reg.by_driver.remove(&driver_key(base));
    Ok(())
}

/// Look up by algorithm name.
pub fn crypto_lookup_alg(name: &str) -> Option<CryptoAlg> {
    REGISTRY.lock().by_name.get(name).copied()
}

/// Look up by driver name.
pub fn crypto_lookup_driver(driver: &str) -> Option<CryptoAlg> {
    REGISTRY.lock().by_driver.get(driver).copied()
}

/// List all registered algorithm names.
pub fn crypto_alg_list() -> Vec<String> {
    REGISTRY.lock().by_name.keys().cloned().collect()
}

/// Count of registered algorithms.
pub fn crypto_alg_count() -> usize {
    REGISTRY.lock().by_name.len()
}

/// Produce `/proc/crypto`-style text for the running registry.
pub fn get_proc_crypto_info() -> String {
    let reg = REGISTRY.lock();
    let mut info = String::from("name         type         driver       priority\n");
    for (name, alg) in reg.by_name.iter() {
        let (algo_type, driver, priority) = match alg {
            CryptoAlg::Hash(h) => ("hash", h.base.driver_name, h.base.priority),
            CryptoAlg::Cipher(c) => ("cipher", c.base.driver_name, c.base.priority),
        };
        info.push_str(&alloc::format!(
            "{:<12} {:<12} {:<12} {}\n",
            name,
            algo_type,
            driver,
            priority
        ));
    }
    info
}
