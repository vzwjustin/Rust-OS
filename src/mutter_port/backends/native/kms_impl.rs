//! KMS Implementation backend selection and management.
//!
//! Abstracts different KMS update mechanisms (simple, atomic, dummy).
//! Selects and coordinates the appropriate implementation based on
//! kernel capabilities.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-impl.h
//! Note: Upstream header not found; minimal stub.

use alloc::vec::Vec;

/// KMS implementation backend selector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaKmsImplType {
    /// Atomic modeset operations
    Atomic,
    /// Simple non-atomic modeset
    Simple,
    /// Dummy (no-op) implementation
    Dummy,
}

/// KMS implementation backend
pub struct MetaKmsImpl {
    /// Implementation type
    pub impl_type: MetaKmsImplType,
}

impl MetaKmsImpl {
    /// Create a new KMS implementation
    pub fn new(impl_type: MetaKmsImplType) -> Self {
        MetaKmsImpl { impl_type }
    }
}

impl Default for MetaKmsImpl {
    fn default() -> Self {
        Self::new(MetaKmsImplType::Simple)
    }
}