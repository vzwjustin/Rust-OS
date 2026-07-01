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
    /// IDs of planes with pending property updates in this batch.
    pub pending_planes: Vec<u32>,
    /// IDs of connectors with pending property updates in this batch.
    pub pending_connectors: Vec<u32>,
    /// Whether this update will be submitted as an atomic commit
    /// (as opposed to a legacy non-atomic commit). Upstream Mutter
    /// sets this based on `DRM_CLIENT_CAP_ATOMIC` support.
    pub is_atomic: bool,
}

impl KmsUpdate {
    /// Create a new update
    pub fn new() -> Self {
        KmsUpdate {
            changes: Vec::new(),
            test_only: false,
            synchronous: false,
            pending_planes: Vec::new(),
            pending_connectors: Vec::new(),
            is_atomic: true,
        }
    }

    /// Add a property change to this update.
    ///
    /// Also records the affected object id in the appropriate pending
    /// list (`pending_planes` for plane changes, `pending_connectors`
    /// for connector changes) so callers can quickly determine which
    /// objects are touched by the batch.
    pub fn add_property_change(&mut self, change: PropertyChange) {
        match change.obj_type {
            PropertyType::Plane => {
                if !self.pending_planes.contains(&change.obj_id) {
                    self.pending_planes.push(change.obj_id);
                }
            }
            PropertyType::Connector => {
                if !self.pending_connectors.contains(&change.obj_id) {
                    self.pending_connectors.push(change.obj_id);
                }
            }
            _ => {}
        }
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

    /// Clear all changes and pending object lists.
    pub fn clear(&mut self) {
        self.changes.clear();
        self.pending_planes.clear();
        self.pending_connectors.clear();
    }

    /// Get the IDs of planes with pending updates in this batch.
    pub fn get_pending_planes(&self) -> &[u32] {
        &self.pending_planes
    }

    /// Get the IDs of connectors with pending updates in this batch.
    pub fn get_pending_connectors(&self) -> &[u32] {
        &self.pending_connectors
    }

    /// Check whether this update will be submitted atomically.
    pub fn is_atomic_commit(&self) -> bool {
        self.is_atomic
    }

    /// Set whether this update will be submitted atomically.
    pub fn set_atomic(&mut self, is_atomic: bool) {
        self.is_atomic = is_atomic;
    }

    /// Apply this update via atomic commit.
    ///
    /// A full implementation would build a DRM atomic request from the
    /// queued `PropertyChange`s and call `drmModeAtomicCommit`. Here we
    /// validate that the batch is non-empty and return `Ok(())` so
    /// callers can exercise the batching logic.
    pub fn commit(&self) -> Result<(), String> {
        if self.is_empty() {
            return Err("No changes to commit".to_string());
        }
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

    #[test]
    fn test_pending_planes_tracking() {
        let mut update = KmsUpdate::new();
        update.add_property_change(PropertyChange::new(PropertyType::Plane, 10, 100, 1));
        update.add_property_change(PropertyChange::new(PropertyType::Plane, 10, 101, 2));
        update.add_property_change(PropertyChange::new(PropertyType::Plane, 11, 102, 3));
        assert_eq!(update.get_pending_planes(), &[10, 11]);
    }

    #[test]
    fn test_pending_connectors_tracking() {
        let mut update = KmsUpdate::new();
        update.add_property_change(PropertyChange::new(PropertyType::Connector, 20, 200, 1));
        update.add_property_change(PropertyChange::new(PropertyType::Connector, 21, 201, 2));
        assert_eq!(update.get_pending_connectors(), &[20, 21]);
    }

    #[test]
    fn test_is_atomic_default_and_override() {
        let mut update = KmsUpdate::new();
        assert!(update.is_atomic_commit());
        update.set_atomic(false);
        assert!(!update.is_atomic_commit());
    }

    #[test]
    fn test_clear_resets_pending_lists() {
        let mut update = KmsUpdate::new();
        update.add_property_change(PropertyChange::new(PropertyType::Plane, 10, 100, 1));
        update.add_property_change(PropertyChange::new(PropertyType::Connector, 20, 200, 1));
        update.clear();
        assert!(update.get_pending_planes().is_empty());
        assert!(update.get_pending_connectors().is_empty());
    }
}
