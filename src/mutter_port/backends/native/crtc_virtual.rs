//! Virtual CRTC implementation for headless/nested modes.
//!
//! Provides a virtual display output when running in nested/headless mode (e.g., in a VM or
//! when using software rendering instead of real hardware). Ported from `meta-crtc-virtual.c`.

use super::crtc_native::{CrtcNative, MonitorTransform};

/// Virtual CRTC with standard capabilities
#[derive(Debug)]
pub struct CrtcVirtual {
    /// Base native CRTC
    pub native: CrtcNative,
}

impl CrtcVirtual {
    /// Virtual CRTC ID bit flag (MSB set)
    const ID_BIT: u64 = 1u64 << 63;

    /// Create a new virtual CRTC
    pub fn new(id: u64) -> Self {
        let virtual_id = Self::ID_BIT | (id & !Self::ID_BIT);
        CrtcVirtual {
            native: CrtcNative::new(virtual_id),
        }
    }

    /// Check if this is a virtual CRTC
    pub fn is_virtual(&self) -> bool {
        (self.native.id & Self::ID_BIT) != 0
    }

    /// Get the virtual ID (without the ID_BIT marker)
    pub fn virtual_id(&self) -> u64 {
        self.native.id & !Self::ID_BIT
    }

    /// Virtual CRTCs support normal rotation but not complex transforms
    pub fn is_transform_handled(&self, transform: MonitorTransform) -> bool {
        transform == MonitorTransform::Normal
    }

    /// Virtual CRTCs report cursor support (for consistency)
    pub fn is_hw_cursor_supported(&self) -> bool {
        true
    }

    /// Virtual CRTCs have no deadline evasion time
    pub fn get_deadline_evasion(&self) -> i64 {
        0
    }

    /// Get gamma LUT size (virtual CRTCs don't support gamma correction)
    pub fn get_gamma_lut_size(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_crtc_creation() {
        let crtc = CrtcVirtual::new(1);
        assert!(crtc.is_virtual());
        assert_eq!(crtc.virtual_id(), 1);
    }

    #[test]
    fn test_virtual_id_bit_set() {
        let crtc = CrtcVirtual::new(0x42);
        let expected_id = (1u64 << 63) | 0x42;
        assert_eq!(crtc.native.id, expected_id);
    }

    #[test]
    fn test_transform_normal_only() {
        let crtc = CrtcVirtual::new(1);
        assert!(crtc.is_transform_handled(MonitorTransform::Normal));
        assert!(!crtc.is_transform_handled(MonitorTransform::Rotated90));
    }

    #[test]
    fn test_hw_cursor_supported() {
        let crtc = CrtcVirtual::new(1);
        assert!(crtc.is_hw_cursor_supported());
    }
}
