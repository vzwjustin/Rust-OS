//! KMS-based GPU device representation.
//!
//! Manages graphics devices via the Linux DRM/KMS subsystem. Handles device enumeration,
//! hotplug, and mode management. Ported from `meta-gpu-kms.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::crtc_kms::CrtcKms;
use super::output_kms::OutputKms;

/// Handle to a KMS device (opaque reference to kernel device)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KmsDeviceHandle(u64);

/// GPU device information
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// Device ID or index
    pub id: u32,
    /// Device file path (e.g., "/dev/dri/card0")
    pub file_path: String,
}

/// KMS GPU device
#[derive(Debug)]
pub struct GpuKms {
    /// GPU device information
    pub info: GpuInfo,
    /// Reference to underlying KMS device
    pub kms_device: Option<KmsDeviceHandle>,
    /// Whether this is the boot VGA device
    pub is_boot_vga: bool,
    /// Whether this is a platform device (not PCI)
    pub is_platform_device: bool,
    /// Associated CRTCs
    pub crtcs: Vec<CrtcKms>,
    /// Associated outputs
    pub outputs: Vec<OutputKms>,
}

impl GpuKms {
    /// Create a new KMS GPU device
    pub fn new(id: u32, file_path: String) -> Self {
        GpuKms {
            info: GpuInfo { id, file_path },
            kms_device: None,
            is_boot_vga: false,
            is_platform_device: false,
            crtcs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    /// Set the KMS device handle
    pub fn set_kms_device(&mut self, device: KmsDeviceHandle) {
        self.kms_device = Some(device);
    }

    /// Get the KMS device handle
    pub fn get_kms_device(&self) -> Option<KmsDeviceHandle> {
        self.kms_device
    }

    /// Set whether this is the boot VGA device
    pub fn set_boot_vga(&mut self, is_boot: bool) {
        self.is_boot_vga = is_boot;
    }

    /// Get the device ID
    pub fn get_id(&self) -> u32 {
        self.info.id
    }

    /// Get the device file path
    pub fn get_file_path(&self) -> &str {
        &self.info.file_path
    }

    /// Check if this is the boot VGA device
    pub fn is_boot_vga_device(&self) -> bool {
        self.is_boot_vga
    }

    /// Check if this is a platform device
    pub fn is_platform_dev(&self) -> bool {
        self.is_platform_device
    }

    /// Set platform device flag
    pub fn set_platform_device(&mut self, is_platform: bool) {
        self.is_platform_device = is_platform;
    }

    /// Check if any CRTC is currently active
    pub fn has_active_crtc(&self) -> bool {
        self.crtcs.iter().any(|c| c.native.active)
    }

    /// Get number of CRTCs
    pub fn get_crtc_count(&self) -> usize {
        self.crtcs.len()
    }

    /// Get number of outputs
    pub fn get_output_count(&self) -> usize {
        self.outputs.len()
    }

    /// Add a CRTC to this device
    pub fn add_crtc(&mut self, crtc: CrtcKms) {
        self.crtcs.push(crtc);
    }

    /// Add an output to this device
    pub fn add_output(&mut self, output: OutputKms) {
        self.outputs.push(output);
    }

    /// Get CRTC by ID
    pub fn get_crtc(&self, id: u64) -> Option<&CrtcKms> {
        self.crtcs.iter().find(|c| c.native.id == id)
    }

    /// Get mutable CRTC by ID
    pub fn get_crtc_mut(&mut self, id: u64) -> Option<&mut CrtcKms> {
        self.crtcs.iter_mut().find(|c| c.native.id == id)
    }

    /// Get output by ID
    pub fn get_output(&self, id: u64) -> Option<&OutputKms> {
        self.outputs.iter().find(|o| o.native.id == id)
    }

    /// Get mutable output by ID
    pub fn get_output_mut(&mut self, id: u64) -> Option<&mut OutputKms> {
        self.outputs.iter_mut().find(|o| o.native.id == id)
    }

    /// Check if device can have outputs (has CRTCs and is functional)
    pub fn can_have_outputs(&self) -> bool {
        !self.crtcs.is_empty()
    }

    /// Read current display configuration from hardware
    /// This requires querying KMS to get current state
    pub fn read_current_config(&mut self) {
        // TODO: Issue DRM ioctl to query current CRTC/output configuration
        // This is a complex operation requiring DRM property queries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_creation() {
        let gpu = GpuKms::new(0, "/dev/dri/card0".to_string());
        assert_eq!(gpu.get_id(), 0);
        assert_eq!(gpu.get_file_path(), "/dev/dri/card0");
        assert!(!gpu.is_boot_vga_device());
    }

    #[test]
    fn test_boot_vga() {
        let mut gpu = GpuKms::new(0, "/dev/dri/card0".to_string());
        assert!(!gpu.is_boot_vga_device());
        gpu.set_boot_vga(true);
        assert!(gpu.is_boot_vga_device());
    }

    #[test]
    fn test_device_handle() {
        let mut gpu = GpuKms::new(0, "/dev/dri/card0".to_string());
        assert_eq!(gpu.get_kms_device(), None);
        let handle = KmsDeviceHandle(42);
        gpu.set_kms_device(handle);
        assert_eq!(gpu.get_kms_device(), Some(handle));
    }

    #[test]
    fn test_crtc_management() {
        let mut gpu = GpuKms::new(0, "/dev/dri/card0".to_string());
        assert_eq!(gpu.get_crtc_count(), 0);
        assert!(!gpu.has_active_crtc());
        assert!(!gpu.can_have_outputs());

        let crtc = CrtcKms::new(1);
        gpu.add_crtc(crtc);
        assert_eq!(gpu.get_crtc_count(), 1);
        assert!(gpu.can_have_outputs());
    }

    #[test]
    fn test_output_management() {
        let mut gpu = GpuKms::new(0, "/dev/dri/card0".to_string());
        let output = OutputKms::new(1, super::super::output_native::ConnectorType::Hdmi);
        gpu.add_output(output);
        assert_eq!(gpu.get_output_count(), 1);
    }
}
