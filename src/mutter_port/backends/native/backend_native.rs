//! Native backend implementation for RustOS.
//!
//! Main backend interface for hardware display management.
//! Ported from `meta-backend-native.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::kms::Kms;

/// Native backend for hardware display control
#[derive(Debug)]
pub struct BackendNative {
    /// KMS subsystem
    pub kms: Kms,
    /// Whether backend is initialized
    pub initialized: bool,
}

impl BackendNative {
    /// Create a new native backend
    pub fn new() -> Self {
        BackendNative {
            kms: Kms::new(),
            initialized: false,
        }
    }

    /// Initialize the backend
    pub fn initialize(&mut self) -> Result<(), String> {
        self.kms.initialize()?;
        self.kms.discover_resources()?;
        self.initialized = true;
        Ok(())
    }

    /// Check if backend is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get mutable reference to KMS subsystem
    pub fn get_kms_mut(&mut self) -> &mut Kms {
        &mut self.kms
    }

    /// Get reference to KMS subsystem
    pub fn get_kms(&self) -> &Kms {
        &self.kms
    }
}

impl Default for BackendNative {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation() {
        let backend = BackendNative::new();
        assert!(!backend.is_initialized());
    }

    #[test]
    fn test_backend_initialization() {
        let mut backend = BackendNative::new();
        let result = backend.initialize();
        assert!(result.is_ok());
        assert!(backend.is_initialized());
    }
}
