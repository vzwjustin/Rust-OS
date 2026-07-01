//! fs-verity filesystem integrity verification stub.
//!
//! Provides a stub for fs-verity, a Linux kernel feature that protects file
//! integrity using Merkle trees and digital signatures. Files can be marked
//! for verification, allowing efficient content integrity checks.
//! See linux-master fs/verity/ for reference.

// TODO: port from linux-master fs/verity/

/// fs-verity hash algorithm (stub).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerityHashAlgorithm {
    /// SHA-256 hash (stub)
    Sha256,
    /// SHA-512 hash (stub)
    Sha512,
}

/// fs-verity descriptor (stub).
#[derive(Debug, Clone)]
pub struct VerityDescriptor {
    /// Hash algorithm (stub)
    pub hash_algorithm: VerityHashAlgorithm,
    /// Root hash (stub)
    pub root_hash: alloc::vec::Vec<u8>,
    /// File size (stub)
    pub file_size: u64,
}

/// Initialize fs-verity subsystem (stub).
pub fn init() -> crate::fs::FsResult<()> {
    // TODO: port from linux-master fs/verity/init.c fsverity_init()
    Ok(())
}

/// Enable fs-verity on a file (stub).
pub fn enable_verity(_inode: crate::fs::InodeNumber, _descriptor: &VerityDescriptor) -> crate::fs::FsResult<()> {
    // TODO: port from linux-master fs/verity/enable.c fsverity_ioctl_enable()
    Err(crate::fs::FsError::NotSupported)
}
