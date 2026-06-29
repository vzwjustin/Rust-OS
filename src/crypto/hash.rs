//! Hash algorithm trait and registry-backed wrappers.

use super::algapi::{crypto_lookup_alg, CryptoAlg, CryptoError};
use super::sha256::{self, Sha256};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// Supported kernel hash algorithm names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashType {
    Sha256,
}

impl HashType {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
        }
    }

    pub const fn digest_size(self) -> usize {
        match self {
            Self::Sha256 => sha256::DIGEST_SIZE,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "sha256" => Some(Self::Sha256),
            _ => None,
        }
    }
}

/// Incremental hash interface (mirrors Linux `shash` update/final).
pub trait Hasher {
    fn update(&mut self, data: &[u8]);
    fn finalize(self) -> Vec<u8>;
    fn digest_size(&self) -> usize;
}

/// SHA-256 hasher exposed through the trait.
pub struct Sha256Hasher {
    inner: Sha256,
}

impl Sha256Hasher {
    pub fn new() -> Self {
        Self {
            inner: Sha256::new(),
        }
    }
}

impl Hasher for Sha256Hasher {
    fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    fn finalize(self) -> Vec<u8> {
        self.inner.finalize().to_vec()
    }

    fn digest_size(&self) -> usize {
        sha256::DIGEST_SIZE
    }
}

/// Fixed-size digest container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashDigest {
    pub algorithm: HashType,
    pub bytes: Vec<u8>,
}

impl HashDigest {
    pub fn to_hex(&self) -> String {
        self.bytes
            .iter()
            .map(|b| alloc::format!("{:02x}", b))
            .collect()
    }

    pub fn verify(&self, data: &[u8]) -> bool {
        hash(self.algorithm, data)
            .map(|d| d.bytes == self.bytes)
            .unwrap_or(false)
    }
}

/// Allocate a hasher for a registered algorithm name.
pub fn alloc_hasher(name: &str) -> Result<Box<dyn Hasher>, CryptoError> {
    let ty = HashType::from_name(name).ok_or(CryptoError::NotFound)?;
    match ty {
        HashType::Sha256 => Ok(Box::new(Sha256Hasher::new())),
    }
}

/// One-shot hash via the algorithm registry.
pub fn hash(algorithm: HashType, data: &[u8]) -> Result<HashDigest, CryptoError> {
    let alg = crypto_lookup_alg(algorithm.name()).ok_or(CryptoError::NotFound)?;
    let digest = match alg {
        CryptoAlg::Hash(h) => (h.hash_one_shot)(data),
        CryptoAlg::Cipher(_) => return Err(CryptoError::NotFound),
    };
    Ok(HashDigest {
        algorithm,
        bytes: digest,
    })
}

/// One-shot hash by registered name string.
pub fn hash_by_name(name: &str, data: &[u8]) -> Result<HashDigest, CryptoError> {
    let ty = HashType::from_name(name).ok_or(CryptoError::NotFound)?;
    hash(ty, data)
}
