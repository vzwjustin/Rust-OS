//! Main KMS (Kernel Mode Setting) subsystem manager.
//!
//! Top-level abstraction for the entire KMS subsystem, managing devices,
//! resource discovery, and atomic updates. Ported from `meta-kms.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::kms_device::{KmsDevice, KmsDeviceFd};
use super::kms_update::{KmsUpdate, PropertyChange, PropertyType};

/// KMS subsystem state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KmsState {
    /// KMS is initialized
    Initialized,
    /// KMS is disabled
    Disabled,
    /// Error occurred
    Error,
}

/// Main KMS manager
#[derive(Debug)]
pub struct Kms {
    /// State
    pub state: KmsState,
    /// Managed KMS devices
    pub devices: Vec<KmsDevice>,
    /// Whether atomic commits are supported
    pub supports_atomic: bool,
    /// Number of page flips submitted but not yet acknowledged by the
    /// kernel. Upstream Mutter tracks this so the compositor can avoid
    /// queueing a second flip while one is already pending on the same
    /// CRTC, and to detect missed vblanks.
    pub pending_page_flips: u32,
    /// Total number of page flips successfully completed since KMS init.
    pub completed_page_flips: u64,
}

impl Kms {
    /// Create a new KMS manager
    pub fn new() -> Self {
        Kms {
            state: KmsState::Disabled,
            devices: Vec::new(),
            supports_atomic: false,
            pending_page_flips: 0,
            completed_page_flips: 0,
        }
    }

    /// Initialize KMS (discover devices, query capabilities)
    ///
    /// A full implementation would enumerate DRM devices via udev or a
    /// `/dev/dri/` directory scan, issue `DRM_IOCTL_VERSION` to verify
    /// kernel support, and set the `DRM_CLIENT_CAP_ATOMIC` capability
    /// when available. In this no_std port we only flip the state flag;
    /// callers register devices explicitly via `add_device`.
    pub fn initialize(&mut self) -> Result<(), String> {
        self.state = KmsState::Initialized;
        Ok(())
    }

    /// Add a device to this KMS manager
    pub fn add_device(&mut self, device: KmsDevice) {
        self.devices.push(device);
    }

    /// Get device by index
    pub fn get_device(&self, index: usize) -> Option<&KmsDevice> {
        self.devices.get(index)
    }

    /// Get mutable device by index
    pub fn get_device_mut(&mut self, index: usize) -> Option<&mut KmsDevice> {
        self.devices.get_mut(index)
    }

    /// Get number of devices
    pub fn get_device_count(&self) -> usize {
        self.devices.len()
    }

    /// Check if atomic commits are supported
    pub fn supports_atomic_commits(&self) -> bool {
        self.supports_atomic
    }

    /// Set atomic commit support flag
    pub fn set_supports_atomic(&mut self, supports: bool) {
        self.supports_atomic = supports;
    }

    /// Get KMS state
    pub fn get_state(&self) -> KmsState {
        self.state
    }

    /// Check if KMS is initialized
    pub fn is_initialized(&self) -> bool {
        self.state == KmsState::Initialized
    }

    /// Discover all KMS resources.
    ///
    /// A full implementation would issue DRM ioctls to enumerate all
    /// resources across all devices. Here we delegate to each device's
    /// `scan_resources` which records the resource counts locally.
    pub fn discover_resources(&mut self) -> Result<(), String> {
        if !self.is_initialized() {
            return Err("KMS not initialized".to_string());
        }

        for device in &mut self.devices {
            device.scan_resources();
        }
        Ok(())
    }

    /// Submit an atomic update.
    ///
    /// A full implementation would batch all updates across devices and
    /// submit atomically via `drmModeAtomicCommit`. Here we record the
    /// pending page flip so callers can track in-flight commits and
    /// delegate to the update's `commit` for validation.
    pub fn submit_update(&mut self, update: &KmsUpdate) -> Result<(), String> {
        if !self.is_initialized() {
            return Err("KMS not initialized".to_string());
        }
        update.commit()?;
        if !update.test_only {
            self.pending_page_flips = self.pending_page_flips.saturating_add(1);
        }
        Ok(())
    }

    /// Acknowledge a completed page flip.
    ///
    /// Called by the backend when the kernel reports a page-flip event.
    /// Decrements the pending counter and bumps the completed counter.
    pub fn page_flip_completed(&mut self) {
        if self.pending_page_flips > 0 {
            self.pending_page_flips -= 1;
        }
        self.completed_page_flips = self.completed_page_flips.saturating_add(1);
    }

    /// Check whether any page flips are currently pending.
    pub fn has_pending_page_flips(&self) -> bool {
        self.pending_page_flips > 0
    }

    /// Get the total number of completed page flips.
    pub fn get_completed_page_flips(&self) -> u64 {
        self.completed_page_flips
    }

    /// Disable all outputs (power down).
    ///
    /// A full implementation would disable all CRTCs via an atomic
    /// commit. Here we clear the pending-flip counter since no flips
    /// can complete on a disabled device.
    pub fn disable_all(&mut self) -> Result<(), String> {
        if !self.is_initialized() {
            return Err("KMS not initialized".to_string());
        }
        self.pending_page_flips = 0;
        Ok(())
    }
}

impl Default for Kms {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kms_creation() {
        let kms = Kms::new();
        assert_eq!(kms.state, KmsState::Disabled);
        assert!(!kms.supports_atomic);
        assert_eq!(kms.get_device_count(), 0);
    }

    #[test]
    fn test_kms_initialization() {
        let mut kms = Kms::new();
        assert!(!kms.is_initialized());
        let result = kms.initialize();
        assert!(result.is_ok());
        assert!(kms.is_initialized());
    }

    #[test]
    fn test_atomic_support() {
        let mut kms = Kms::new();
        assert!(!kms.supports_atomic_commits());
        kms.set_supports_atomic(true);
        assert!(kms.supports_atomic_commits());
    }

    #[test]
    fn test_device_management() {
        let mut kms = Kms::new();
        let device = KmsDevice::new(KmsDeviceFd::new(3), "/dev/dri/card0".to_string());
        kms.add_device(device);
        assert_eq!(kms.get_device_count(), 1);
        assert!(kms.get_device(0).is_some());
    }

    #[test]
    fn test_page_flip_tracking() {
        let mut kms = Kms::new();
        kms.initialize().unwrap();
        assert!(!kms.has_pending_page_flips());
        assert_eq!(kms.get_completed_page_flips(), 0);

        let mut update = KmsUpdate::new();
        update.add_property_change(PropertyChange::new(PropertyType::CRTC, 1, 100, 42));
        kms.submit_update(&update).unwrap();
        assert!(kms.has_pending_page_flips());
        assert_eq!(kms.get_completed_page_flips(), 0);

        kms.page_flip_completed();
        assert!(!kms.has_pending_page_flips());
        assert_eq!(kms.get_completed_page_flips(), 1);
    }

    #[test]
    fn test_test_only_update_does_not_count() {
        let mut kms = Kms::new();
        kms.initialize().unwrap();
        let mut update = KmsUpdate::new();
        update.set_test_only(true);
        update.add_property_change(PropertyChange::new(PropertyType::CRTC, 1, 100, 42));
        kms.submit_update(&update).unwrap();
        assert!(!kms.has_pending_page_flips());
    }

    #[test]
    fn test_disable_all_clears_pending() {
        let mut kms = Kms::new();
        kms.initialize().unwrap();
        kms.pending_page_flips = 3;
        kms.disable_all().unwrap();
        assert!(!kms.has_pending_page_flips());
    }
}
