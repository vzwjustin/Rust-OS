//! KMS atomic update batching for display configuration changes.
//!
//! Batches multiple KMS property changes into a single atomic commit.
//! Ported from `meta-kms-update.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Type of property being updated
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyType {
    CRTC,
    Connector,
    Plane,
    Framebuffer,
}

/// Single property change in an update
#[derive(Debug, Clone)]
pub struct PropertyChange {
    /// Object type (CRTC, connector, plane, etc.)
    pub obj_type: PropertyType,
    /// Object ID
    pub obj_id: u32,
    /// Property ID
    pub prop_id: u32,
    /// Property value
    pub value: u64,
}

impl PropertyChange {
    /// Create a new property change
    pub fn new(obj_type: PropertyType, obj_id: u32, prop_id: u32, value: u64) -> Self {
        PropertyChange {
            obj_type,
            obj_id,
            prop_id,
            value,
        }
    }
}

/// Atomic KMS update - batches multiple property changes
#[derive(Debug, Clone)]
pub struct KmsUpdate {
    /// Changes to apply
    pub changes: Vec<PropertyChange>,
    /// Whether to enable test-only mode (dry-run)
    pub test_only: bool,
    /// Whether to apply synchronously
    pub synchronous: bool,
}

impl KmsUpdate {
    /// Create a new update
    pub fn new() -> Self {
        KmsUpdate {
            changes: Vec::new(),
            test_only: false,
            synchronous: false,
        }
    }

    /// Add a property change to this update
    pub fn add_property_change(&mut self, change: PropertyChange) {
        self.changes.push(change);
    }

    /// Set test-only mode (dry-run without applying)
    pub fn set_test_only(&mut self, test_only: bool) {
        self.test_only = test_only;
    }

    /// Set synchronous mode
    pub fn set_synchronous(&mut self, sync: bool) {
        self.synchronous = sync;
    }

    /// Get number of changes
    pub fn get_change_count(&self) -> usize {
        self.changes.len()
    }

    /// Check if this update is empty
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Clear all changes
    pub fn clear(&mut self) {
        self.changes.clear();
    }

    /// Apply this update via atomic commit
    /// TODO: Issue drmModeAtomicCommit() to kernel
    pub fn commit(&self) -> Result<(), String> {
        if self.is_empty() {
            return Err("No changes to commit".to_string());
        }
        // TODO: Build DRM atomic request and commit
        Ok(())
    }
}

impl Default for KmsUpdate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_creation() {
        let update = KmsUpdate::new();
        assert!(update.is_empty());
        assert_eq!(update.get_change_count(), 0);
    }

    #[test]
    fn test_add_property_change() {
        let mut update = KmsUpdate::new();
        let change = PropertyChange::new(PropertyType::CRTC, 1, 100, 42);
        update.add_property_change(change);
        assert_eq!(update.get_change_count(), 1);
    }

    #[test]
    fn test_test_only_mode() {
        let mut update = KmsUpdate::new();
        assert!(!update.test_only);
        update.set_test_only(true);
        assert!(update.test_only);
    }

    #[test]
    fn test_clear_changes() {
        let mut update = KmsUpdate::new();
        update.add_property_change(PropertyChange::new(PropertyType::CRTC, 1, 100, 42));
        assert_eq!(update.get_change_count(), 1);
        update.clear();
        assert!(update.is_empty());
    }
}
