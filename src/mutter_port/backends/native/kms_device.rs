//! KMS device management for a single DRM device.
//!
//! Manages all KMS objects (CRTCs, connectors, planes) for a single graphics card.
//! Ported from `meta-kms-device.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::kms_connector::KmsConnector;
use super::kms_crtc::KmsCrtc;
use super::kms_plane::KmsPlane;

/// KMS device file descriptor (opaque handle)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KmsDeviceFd(i32);

impl KmsDeviceFd {
    /// Create from raw file descriptor
    pub fn new(fd: i32) -> Self {
        KmsDeviceFd(fd)
    }

    /// Get the raw file descriptor
    pub fn get(&self) -> i32 {
        self.0
    }
}

/// KMS device
#[derive(Debug)]
pub struct KmsDevice {
    /// File descriptor for DRM device
    pub fd: KmsDeviceFd,
    /// Device path (e.g., /dev/dri/card0)
    pub path: String,
    /// Available CRTCs
    pub crtcs: Vec<KmsCrtc>,
    /// Available connectors
    pub connectors: Vec<KmsConnector>,
    /// Available planes
    pub planes: Vec<KmsPlane>,
}

impl KmsDevice {
    /// Create a new KMS device
    pub fn new(fd: KmsDeviceFd, path: String) -> Self {
        KmsDevice {
            fd,
            path,
            crtcs: Vec::new(),
            connectors: Vec::new(),
            planes: Vec::new(),
        }
    }

    /// Get device file descriptor
    pub fn get_fd(&self) -> KmsDeviceFd {
        self.fd
    }

    /// Get device path
    pub fn get_path(&self) -> &str {
        &self.path
    }

    /// Add a CRTC
    pub fn add_crtc(&mut self, crtc: KmsCrtc) {
        self.crtcs.push(crtc);
    }

    /// Add a connector
    pub fn add_connector(&mut self, connector: KmsConnector) {
        self.connectors.push(connector);
    }

    /// Add a plane
    pub fn add_plane(&mut self, plane: KmsPlane) {
        self.planes.push(plane);
    }

    /// Get CRTC by ID
    pub fn get_crtc(&self, id: u32) -> Option<&KmsCrtc> {
        self.crtcs.iter().find(|c| c.id == id)
    }

    /// Get mutable CRTC by ID
    pub fn get_crtc_mut(&mut self, id: u32) -> Option<&mut KmsCrtc> {
        self.crtcs.iter_mut().find(|c| c.id == id)
    }

    /// Get connector by ID
    pub fn get_connector(&self, id: u32) -> Option<&KmsConnector> {
        self.connectors.iter().find(|c| c.id == id)
    }

    /// Get mutable connector by ID
    pub fn get_connector_mut(&mut self, id: u32) -> Option<&mut KmsConnector> {
        self.connectors.iter_mut().find(|c| c.id == id)
    }

    /// Get plane by ID
    pub fn get_plane(&self, id: u32) -> Option<&KmsPlane> {
        self.planes.iter().find(|p| p.id == id)
    }

    /// Get mutable plane by ID
    pub fn get_plane_mut(&mut self, id: u32) -> Option<&mut KmsPlane> {
        self.planes.iter_mut().find(|p| p.id == id)
    }

    /// Get number of CRTCs
    pub fn get_crtc_count(&self) -> usize {
        self.crtcs.len()
    }

    /// Get number of connectors
    pub fn get_connector_count(&self) -> usize {
        self.connectors.len()
    }

    /// Get number of planes
    pub fn get_plane_count(&self) -> usize {
        self.planes.len()
    }

    /// Scan device resources from kernel
    /// TODO: Issue DRM ioctl to query all resources
    pub fn scan_resources(&mut self) {
        // TODO: Call drmModeGetResources(fd) to enumerate CRTCs, connectors, planes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_creation() {
        let device = KmsDevice::new(KmsDeviceFd::new(3), "/dev/dri/card0".to_string());
        assert_eq!(device.fd.get(), 3);
        assert_eq!(device.path, "/dev/dri/card0");
    }

    #[test]
    fn test_crtc_management() {
        let mut device = KmsDevice::new(KmsDeviceFd::new(3), "/dev/dri/card0".to_string());
        device.add_crtc(KmsCrtc::new(1));
        assert_eq!(device.get_crtc_count(), 1);
        assert!(device.get_crtc(1).is_some());
        assert!(device.get_crtc(2).is_none());
    }

    #[test]
    fn test_connector_management() {
        use super::super::kms_connector::ConnectorKmsType;
        let mut device = KmsDevice::new(KmsDeviceFd::new(3), "/dev/dri/card0".to_string());
        device.add_connector(KmsConnector::new(1, ConnectorKmsType::HDMIA, 1));
        assert_eq!(device.get_connector_count(), 1);
    }

    #[test]
    fn test_plane_management() {
        use super::super::kms_plane::PlaneType;
        let mut device = KmsDevice::new(KmsDeviceFd::new(3), "/dev/dri/card0".to_string());
        device.add_plane(KmsPlane::new(1, PlaneType::Primary));
        assert_eq!(device.get_plane_count(), 1);
    }
}
