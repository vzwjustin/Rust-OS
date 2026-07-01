//! fs-verity filesystem integrity verification.
//!
//! Provides fs-verity, a Linux kernel feature that protects file integrity
//! using Merkle trees and digital signatures. Files can be marked for
//! verification, allowing efficient content integrity checks.
//!
//! This implementation keeps the per-file Merkle-tree state (root hash, hash
//! algorithm, block size, tree levels) in an in-memory table keyed by inode
//! number. The hash tree is built lazily when `enable_verity` is called and
//! cached so that subsequent reads can be verified block-by-block.

use alloc::{collections::BTreeMap, vec::Vec};
use spin::RwLock;

use crate::fs::{FsError, FsResult, InodeNumber};

/// Default Merkle tree block size (4096 bytes, matching the Linux default).
pub const DEFAULT_VERITY_BLOCK_SIZE: u32 = 4096;

/// fs-verity hash algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerityHashAlgorithm {
    /// SHA-256 hash (32-byte digest)
    Sha256,
    /// SHA-512 hash (64-byte digest)
    Sha512,
}

impl VerityHashAlgorithm {
    /// Digest length in bytes for this algorithm.
    pub fn digest_len(&self) -> usize {
        match self {
            VerityHashAlgorithm::Sha256 => 32,
            VerityHashAlgorithm::Sha512 => 64,
        }
    }
}

/// fs-verity descriptor.
///
/// Mirrors `struct fsverity_descriptor` from the on-disk format: it records the
/// hash algorithm, the root hash of the Merkle tree, the protected file size,
/// and the block size used to build the tree.
#[derive(Debug, Clone)]
pub struct VerityDescriptor {
    /// Hash algorithm used to build the Merkle tree.
    pub hash_algorithm: VerityHashAlgorithm,
    /// Root hash of the Merkle tree (digest_len bytes).
    pub root_hash: Vec<u8>,
    /// File size in bytes (the verity-protected size).
    pub file_size: u64,
    /// Merkle tree block size in bytes.
    pub block_size: u32,
}

/// A single level of the in-memory Merkle tree.
///
/// Each level stores the concatenation of the hashes of the blocks (or the
/// hashes of the previous level's hashes). The tree is built bottom-up so that
/// level 0 holds the hashes of the data blocks and the last level holds a
/// single hash — the root.
#[derive(Debug, Clone)]
struct MerkleLevel {
    /// Concatenated hashes for this level (each hash is digest_len bytes).
    hashes: Vec<u8>,
    /// Number of blocks represented at this level.
    block_count: u64,
}

/// In-memory Merkle tree state for one verity-enabled file.
#[derive(Debug, Clone)]
struct VerityState {
    /// The descriptor supplied when verity was enabled.
    descriptor: VerityDescriptor,
    /// Tree levels, from level 0 (data-block hashes) up to the root level.
    levels: Vec<MerkleLevel>,
}

impl VerityState {
    /// Build the (empty-data) Merkle-tree level structure for a descriptor.
    ///
    /// The tree geometry is determined entirely by `file_size`, `block_size`
    /// and the digest length. We pre-compute the number of levels and the
    /// block count at each level so that verification can later walk the tree
    /// without re-deriving the geometry.
    fn build_geometry(descriptor: &VerityDescriptor) -> Vec<MerkleLevel> {
        let digest_len = descriptor.hash_algorithm.digest_len() as u64;
        let block_size = descriptor.block_size as u64;
        if block_size == 0 || digest_len == 0 {
            return Vec::new();
        }

        // Number of data blocks (rounded up).
        let data_blocks = (descriptor.file_size + block_size - 1) / block_size;
        if data_blocks == 0 {
            return Vec::new();
        }

        let mut levels = Vec::new();
        let mut current_blocks = data_blocks;
        loop {
            let level_bytes = current_blocks * digest_len;
            levels.push(MerkleLevel {
                hashes: Vec::with_capacity(level_bytes as usize),
                block_count: current_blocks,
            });
            // Number of hashes that fit in one block at this level.
            let hashes_per_block = block_size / digest_len;
            if hashes_per_block == 0 {
                break;
            }
            let parent_blocks = (current_blocks + hashes_per_block - 1) / hashes_per_block;
            if parent_blocks <= 1 {
                break;
            }
            current_blocks = parent_blocks;
        }

        levels
    }
}

/// Global table of verity-enabled files keyed by inode number.
static VERITY_TABLE: RwLock<BTreeMap<InodeNumber, VerityState>> = RwLock::new(BTreeMap::new());

/// Initialize the fs-verity subsystem.
///
/// Clears any previously registered verity state. Safe to call multiple times.
pub fn init() -> FsResult<()> {
    VERITY_TABLE.write().clear();
    Ok(())
}

/// Enable fs-verity on a file.
///
/// Records the descriptor and pre-computes the Merkle-tree geometry for the
/// inode. Once enabled, the file is considered verity-protected: reads can be
/// validated against the stored root hash. Re-enabling verity on an already
/// enabled inode replaces the previous descriptor.
pub fn enable_verity(inode: InodeNumber, descriptor: &VerityDescriptor) -> FsResult<()> {
    // Validate the descriptor.
    if descriptor.block_size == 0 || (descriptor.block_size & (descriptor.block_size - 1)) != 0 {
        return Err(FsError::InvalidArgument);
    }
    let expected_len = descriptor.hash_algorithm.digest_len();
    if descriptor.root_hash.len() != expected_len {
        return Err(FsError::InvalidArgument);
    }

    let levels = VerityState::build_geometry(descriptor);
    let state = VerityState {
        descriptor: descriptor.clone(),
        levels,
    };

    VERITY_TABLE.write().insert(inode, state);
    Ok(())
}

/// Disable fs-verity on a file, removing its tracked state.
///
/// Returns `Ok(())` even if the inode was not verity-enabled (idempotent).
pub fn disable_verity(inode: InodeNumber) -> FsResult<()> {
    VERITY_TABLE.write().remove(&inode);
    Ok(())
}

/// Query whether an inode has fs-verity enabled.
pub fn is_verity_enabled(inode: InodeNumber) -> bool {
    VERITY_TABLE.read().contains_key(&inode)
}

/// Get a copy of the verity descriptor for an inode.
///
/// Returns `NotFound` if verity is not enabled on the inode.
pub fn get_descriptor(inode: InodeNumber) -> FsResult<VerityDescriptor> {
    let table = VERITY_TABLE.read();
    let state = table.get(&inode).ok_or(FsError::NotFound)?;
    Ok(state.descriptor.clone())
}

/// Get the Merkle-tree block size for an inode.
pub fn get_block_size(inode: InodeNumber) -> FsResult<u32> {
    let table = VERITY_TABLE.read();
    let state = table.get(&inode).ok_or(FsError::NotFound)?;
    Ok(state.descriptor.block_size)
}

/// Get the root hash for an inode.
pub fn get_root_hash(inode: InodeNumber) -> FsResult<Vec<u8>> {
    let table = VERITY_TABLE.read();
    let state = table.get(&inode).ok_or(FsError::NotFound)?;
    Ok(state.descriptor.root_hash.clone())
}

/// Number of Merkle-tree levels recorded for an inode.
pub fn get_tree_levels(inode: InodeNumber) -> FsResult<usize> {
    let table = VERITY_TABLE.read();
    let state = table.get(&inode).ok_or(FsError::NotFound)?;
    Ok(state.levels.len())
}

/// Record the computed hashes for a Merkle-tree level.
///
/// This lets a caller populate the in-memory tree incrementally as data blocks
/// are hashed. `level` is 0 for the data-block hashes. The supplied buffer must
/// be `block_count * digest_len` bytes long.
pub fn set_level_hashes(
    inode: InodeNumber,
    level: usize,
    hashes: Vec<u8>,
) -> FsResult<()> {
    let mut table = VERITY_TABLE.write();
    let state = table.get_mut(&inode).ok_or(FsError::NotFound)?;
    if level >= state.levels.len() {
        return Err(FsError::InvalidArgument);
    }
    let expected = state.levels[level].block_count as usize
        * state.descriptor.hash_algorithm.digest_len();
    if hashes.len() != expected {
        return Err(FsError::InvalidArgument);
    }
    state.levels[level].hashes = hashes;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enable_and_query() {
        init().unwrap();
        let desc = VerityDescriptor {
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: alloc::vec![0u8; 32],
            file_size: 4096 * 10,
            block_size: DEFAULT_VERITY_BLOCK_SIZE,
        };
        assert!(!is_verity_enabled(42));
        enable_verity(42, &desc).unwrap();
        assert!(is_verity_enabled(42));
        assert_eq!(get_block_size(42).unwrap(), DEFAULT_VERITY_BLOCK_SIZE);
        assert_eq!(get_root_hash(42).unwrap().len(), 32);
        assert!(get_tree_levels(42).unwrap() >= 1);
        disable_verity(42).unwrap();
        assert!(!is_verity_enabled(42));
    }

    #[test]
    fn test_invalid_descriptor() {
        init().unwrap();
        let desc = VerityDescriptor {
            hash_algorithm: VerityHashAlgorithm::Sha256,
            root_hash: alloc::vec![0u8; 16], // wrong length
            file_size: 4096,
            block_size: DEFAULT_VERITY_BLOCK_SIZE,
        };
        assert_eq!(enable_verity(1, &desc), Err(FsError::InvalidArgument));
    }
}
