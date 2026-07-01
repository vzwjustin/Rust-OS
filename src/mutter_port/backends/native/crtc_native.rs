//! Native CRTC abstraction for display output.
//!
//! This module provides the abstract base type for hardware-specific CRTC implementations
//! (Virtual, KMS-based, etc.). Ported from `meta-crtc-native.c`.
//!
//! CRTCs (Cathode Ray Tube Controllers) manage the display output pipeline, handling
//! mode setting, refresh rates, and hardware cursor support.

/// Monitor transform types (rotation, flipping)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorTransform {
    Normal,
    Rotated90,
    Rotated180,
    Rotated270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

/// Abstract base for native CRTC implementations
#[derive(Debug)]
pub struct CrtcNative {
    /// Unique CRTC identifier
    pub id: u64,
    /// Whether this CRTC is active/enabled
    pub active: bool,
}

impl CrtcNative {
    /// Create a new CRTC with the given ID
    pub fn new(id: u64) -> Self {
        CrtcNative { id, active: false }
    }

    /// Check if the given display transform is handled by hardware
    pub fn is_transform_handled(&self, transform: MonitorTransform) -> bool {
        // Default: subclasses override this
        transform == MonitorTransform::Normal
    }

    /// Check if hardware cursor is supported on this CRTC
    pub fn is_hw_cursor_supported(&self) -> bool {
        // Default: subclasses override this
        false
    }

    /// Get deadline evasion time in microseconds
    /// This represents how much time before the deadline the kernel needs to start
    pub fn get_deadline_evasion(&self) -> i64 {
        // Default: subclasses override this
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crtc_creation() {
        let crtc = CrtcNative::new(42);
        assert_eq!(crtc.id, 42);
        assert!(!crtc.active);
    }

    #[test]
    fn test_transform_normal_handled() {
        let crtc = CrtcNative::new(1);
        assert!(crtc.is_transform_handled(MonitorTransform::Normal));
    }
}
