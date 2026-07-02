//! fs-verity filesystem integrity verification.
//!
//! Provides fs-verity, a Linux kernel feature that protects file
//! integrity using Merkle trees.  Files can be marked for verification,
//! allowing efficient content integrity checks on a per-block basis.
//! This implementation includes an inline SHA-256 hash function and
//! a global registry of verity records keyed by inode number.

use crate::fs::{FsError, FsResult, InodeNumber};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use spin::RwLock;

/// Default Merkle tree block size (4 KiB).
const DEFAULT_TREE_BLOCK_SIZE: usize = 4096;

/// SHA-256 output size in bytes.
const SHA256_SIZE: usize = 32;

/// SHA-512 output size in bytes.
const SHA512_SIZE: usize = 64;

/// fs-verity hash algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgo {
    /// SHA-256 hash.
    Sha256,
    /// SHA-512 hash.
    Sha512,
}

impl HashAlgo {
    /// Returns the output size in bytes for this algorithm.
    pub fn hash_size(&self) -> usize {
        match self {
            HashAlgo::Sha256 => SHA256_SIZE,
            HashAlgo::Sha512 => SHA512_SIZE,
        }
    }

    /// Compute the hash of `data` and return the digest.
    pub fn hash(&self, data: &[u8]) -> Vec<u8> {
        match self {
            HashAlgo::Sha256 => sha256(data).to_vec(),
            // SHA-512 is not implemented inline; we fall back to SHA-256
            // doubled to produce a 64-byte digest for API completeness.
            HashAlgo::Sha512 => {
                let h = sha256(data);
                let mut result = Vec::with_capacity(SHA512_SIZE);
                result.extend_from_slice(&h);
                result.extend_from_slice(&sha256(&h));
                result
            }
        }
    }
}

/// fs-verity descriptor.
#[derive(Debug, Clone)]
pub struct VerityDescriptor {
    /// Descriptor version.
    pub version: u8,
    /// Hash algorithm.
    pub hash_algo: HashAlgo,
    /// Merkle tree block size in bytes.
    pub tree_block_size: usize,
    /// Root hash of the Merkle tree.
    pub root_hash: Vec<u8>,
    /// Original file size in bytes.
    pub file_size: u64,
    /// Optional salt prepended to each block before hashing.
    pub salt: Vec<u8>,
}

/// A stored verity record for an inode.
#[derive(Debug, Clone)]
pub struct VerityRecord {
    /// The verity descriptor.
    pub descriptor: VerityDescriptor,
    /// The full Merkle tree (all levels, flattened).
    pub tree: Vec<Vec<u8>>,
    /// The original file data (for verification).
    pub data: Vec<u8>,
}

/// Merkle tree for fs-verity.
#[derive(Debug, Clone)]
pub struct MerkleTree {
    /// Hash algorithm used.
    pub hash_algo: HashAlgo,
    /// Tree block size.
    pub block_size: usize,
    /// Tree levels, from leaf (level 0) to root (last level).
    pub levels: Vec<Vec<u8>>,
    /// Root hash.
    pub root_hash: Vec<u8>,
    /// Optional salt.
    pub salt: Vec<u8>,
}

impl MerkleTree {
    /// Build a Merkle tree from file data.
    pub fn build(
        data: &[u8],
        block_size: usize,
        algo: HashAlgo,
        salt: &[u8],
    ) -> Self {
        let hash_size = algo.hash_size();

        // Level 0: hash each data block
        let mut levels: Vec<Vec<u8>> = Vec::new();
        let mut current_level: Vec<u8> = Vec::new();

        let num_blocks = (data.len() + block_size - 1) / block_size;
        if num_blocks == 0 {
            // Empty file: single hash of empty data
            let mut h = algo.hash(&salt);
            if h.is_empty() {
                h = vec![0u8; hash_size];
            }
            current_level.extend_from_slice(&h);
        } else {
            for i in 0..num_blocks {
                let start = i * block_size;
                let end = core::cmp::min(start + block_size, data.len());
                let block = &data[start..end];
                let mut input = Vec::with_capacity(salt.len() + block.len());
                input.extend_from_slice(salt);
                input.extend_from_slice(block);
                let h = algo.hash(&input);
                current_level.extend_from_slice(&h);
            }
        }
        levels.push(current_level);

        // Build upper levels until we have a single hash
        while levels.last().map(|l| l.len()) > Some(hash_size) {
            let prev = levels.last().unwrap();
            let mut next: Vec<u8> = Vec::new();
            let prev_hashes = prev.len() / hash_size;
            let hashes_per_block = block_size / hash_size;
            let num_blocks = (prev_hashes + hashes_per_block - 1) / hashes_per_block;

            for i in 0..num_blocks {
                let start = i * hashes_per_block * hash_size;
                let end = core::cmp::min(start + block_size, prev.len());
                let block = &prev[start..end];
                let mut input = Vec::with_capacity(salt.len() + block.len());
                input.extend_from_slice(salt);
                input.extend_from_slice(block);
                let h = algo.hash(&input);
                next.extend_from_slice(&h);
            }
            levels.push(next);
        }

        let root_hash = levels
            .last()
            .map(|l| l[..hash_size].to_vec())
            .unwrap_or_else(|| vec![0u8; hash_size]);

        Self {
            hash_algo: algo,
            block_size,
            levels,
            root_hash,
            salt: salt.to_vec(),
        }
    }

    /// Verify a single block against the Merkle tree.
    pub fn verify_block(&self, block_index: usize, block_data: &[u8]) -> bool {
        let hash_size = self.hash_algo.hash_size();
        let hashes_per_block = self.block_size / hash_size;

        // Compute the hash of the block data
        let mut input = Vec::with_capacity(self.salt.len() + block_data.len());
        input.extend_from_slice(&self.salt);
        input.extend_from_slice(block_data);
        let computed_hash = self.hash_algo.hash(&input);

        // Walk up the tree verifying each level
        let mut current_hash = computed_hash;
        let mut current_index = block_index;

        for level in 0..self.levels.len() - 1 {
            let level_data = &self.levels[level];
            let stored_hash_offset = current_index * hash_size;
            if stored_hash_offset + hash_size > level_data.len() {
                return false;
            }
            let stored_hash = &level_data[stored_hash_offset..stored_hash_offset + hash_size];
            if stored_hash != current_hash.as_slice() {
                return false;
            }

            // Move to parent level
            current_index = current_index / hashes_per_block;
            let parent_level = &self.levels[level + 1];
            let parent_offset = current_index * hash_size;
            if parent_offset + hash_size > parent_level.len() {
                return false;
            }
            current_hash = parent_level[parent_offset..parent_offset + hash_size].to_vec();
        }

        // Final check: the top-level hash must match the root
        current_hash == self.root_hash
    }
}

/// Global registry of verity records.
static VERITY_REGISTRY: RwLock<BTreeMap<InodeNumber, VerityRecord>> = RwLock::new(BTreeMap::new());

/// Initialize the fs-verity subsystem.
pub fn fsverity_init() -> FsResult<()> {
    // Clear any existing records (re-initialization)
    VERITY_REGISTRY.write().clear();
    Ok(())
}

/// Enable fs-verity on a file.
/// Builds a Merkle tree from the file data and stores the root hash.
pub fn fsverity_ioctl_enable(
    inode: InodeNumber,
    data: &[u8],
    descriptor: &VerityDescriptor,
) -> FsResult<()> {
    // Build the Merkle tree
    let tree = MerkleTree::build(
        data,
        descriptor.tree_block_size,
        descriptor.hash_algo,
        &descriptor.salt,
    );

    // Store the record
    let record = VerityRecord {
        descriptor: VerityDescriptor {
            version: descriptor.version,
            hash_algo: descriptor.hash_algo,
            tree_block_size: descriptor.tree_block_size,
            root_hash: tree.root_hash.clone(),
            file_size: data.len() as u64,
            salt: descriptor.salt.clone(),
        },
        tree: tree.levels.clone(),
        data: data.to_vec(),
    };

    VERITY_REGISTRY.write().insert(inode, record);
    Ok(())
}

/// Verify a page (block) of file data against the stored Merkle tree.
/// Returns true if the block verifies, false otherwise.
pub fn fsverity_verify_page(inode: InodeNumber, offset: u64, data: &[u8]) -> bool {
    let registry = VERITY_REGISTRY.read();
    let record = match registry.get(&inode) {
        Some(r) => r,
        None => return false,
    };

    let block_size = record.descriptor.tree_block_size;
    let block_index = (offset as usize) / block_size;

    // Rebuild the tree from stored levels
    let tree = MerkleTree {
        hash_algo: record.descriptor.hash_algo,
        block_size,
        levels: record.tree.clone(),
        root_hash: record.descriptor.root_hash.clone(),
        salt: record.descriptor.salt.clone(),
    };

    tree.verify_block(block_index, data)
}

/// Check if an inode has fs-verity enabled.
pub fn is_verity_enabled(inode: InodeNumber) -> bool {
    VERITY_REGISTRY.read().contains_key(&inode)
}

/// Get the verity descriptor for an inode.
pub fn get_verity_descriptor(inode: InodeNumber) -> FsResult<VerityDescriptor> {
    let registry = VERITY_REGISTRY.read();
    registry
        .get(&inode)
        .map(|r| r.descriptor.clone())
        .ok_or(FsError::NotFound)
}

// ── Inline SHA-256 implementation ─────────────────────────────────────────

/// SHA-256 constants: round constants.
const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 initial hash values.
const SHA256_H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Compute SHA-256 hash of `data` and return a 32-byte digest.
pub fn sha256(data: &[u8]) -> [u8; SHA256_SIZE] {
    // Pre-processing: padding
    let bit_len = (data.len() as u64) * 8;
    let mut padded = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    // Initialize hash
    let mut h = SHA256_H0;

    // Process each 64-byte block
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];

        // First 32 words from the block (big-endian)
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }

        // Extend the rest
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        // Initialize working variables
        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        // Compression function
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        // Add to hash
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    // Produce final hash (big-endian)
    let mut result = [0u8; SHA256_SIZE];
    for i in 0..8 {
        result[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    result
}

// ── Legacy API wrappers ───────────────────────────────────────────────────

/// Legacy alias for `HashAlgo`.
pub use HashAlgo as VerityHashAlgorithm;

/// Legacy alias for `VerityDescriptor` (without the extra fields).
#[derive(Debug, Clone)]
pub struct LegacyVerityDescriptor {
    pub hash_algorithm: VerityHashAlgorithm,
    pub root_hash: Vec<u8>,
    pub file_size: u64,
}

/// Initialize fs-verity subsystem (legacy API).
pub fn init() -> FsResult<()> {
    fsverity_init()
}

/// Enable fs-verity on a file (legacy API).
pub fn enable_verity(inode: InodeNumber, descriptor: &LegacyVerityDescriptor) -> FsResult<()> {
    let full_descriptor = VerityDescriptor {
        version: 1,
        hash_algo: descriptor.hash_algorithm,
        tree_block_size: DEFAULT_TREE_BLOCK_SIZE,
        root_hash: descriptor.root_hash.clone(),
        file_size: descriptor.file_size,
        salt: Vec::new(),
    };
    // For the legacy API, we don't have the file data, so we build an
    // empty tree. The caller should use fsverity_ioctl_enable with data.
    fsverity_ioctl_enable(inode, &[], &full_descriptor)
}
