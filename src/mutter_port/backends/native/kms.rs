//! Main KMS (Kernel Mode Setting) subsystem manager.
//!
//! Top-level abstraction for the entire KMS subsystem, managing devices,
//! resource discovery, and atomic updates. Ported from `meta-kms.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::kms_device::{KmsDevice, KmsDeviceFd};
use super::kms_update::KmsUpdate;

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
}

impl Kms {
    /// Create a new KMS manager
    pub fn new() -> Self {
        Kms {
            state: KmsState::Disabled,
            devices: Vec::new(),
            supports_atomic: false,
        }
    }

    /// Initialize KMS (discover devices, query capabilities)
    pub fn initialize(&mut self) -> Result<(), String> {
        // TODO: Enumerate DRM devices via udev or /dev/dri/ directory scan
        // TODO: Query DRM_IOCTL_VERSION to check kernel support
        // TODO: Set atomic DRM client capability if available
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

    /// Discover all KMS resources
    /// TODO: Issue DRM ioctls to enumerate all resources across all devices
    pub fn discover_resources(&mut self) -> Result<(), String> {
        if !self.is_initialized() {
            return Err("KMS not initialized".to_string());
        }

        for device in &mut self.devices {
            device.scan_resources();
        }
        Ok(())
    }

    /// Submit an atomic update
    /// TODO: Batch all updates across devices and submit atomically
    pub fn submit_update(&self, update: &KmsUpdate) -> Result<(), String> {
        if !self.is_initialized() {
            return Err("KMS not initialized".to_string());
        }
        update.commit()
    }

    /// Disable all outputs (power down)
    /// TODO: Disable all CRTCs via atomic commit
    pub fn disable_all(&mut self) -> Result<(), String> {
        if !self.is_initialized() {
            return Err("KMS not initialized".to_string());
        }
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
}
