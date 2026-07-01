//! KMS CRTC (display pipeline) management.
//!
//! Core KMS CRTC object representing a hardware display controller.
//! Ported from `meta-kms-crtc.c`.

use crate::alloc::vec::Vec;

/// CRTC state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrtcState {
    /// Active and displaying
    Active,
    /// Disabled
    Inactive,
}

/// KMS CRTC object
#[derive(Debug)]
pub struct KmsCrtc {
    /// CRTC ID from kernel
    pub id: u32,
    /// Current state
    pub state: CrtcState,
    /// Connected output ID if any
    pub current_output_id: Option<u32>,
    /// Current mode if active
    pub current_mode: Option<u32>,
}

impl KmsCrtc {
    /// Create a new CRTC
    pub fn new(id: u32) -> Self {
        KmsCrtc {
            id,
            state: CrtcState::Inactive,
            current_output_id: None,
            current_mode: None,
        }
    }

    /// Activate this CRTC
    pub fn activate(&mut self) {
        self.state = CrtcState::Active;
    }

    /// Deactivate this CRTC
    pub fn deactivate(&mut self) {
        self.state = CrtcState::Inactive;
    }

    /// Check if CRTC is active
    pub fn is_active(&self) -> bool {
        self.state == CrtcState::Active
    }

    /// Set the current output
    pub fn set_current_output(&mut self, output_id: u32) {
        self.current_output_id = Some(output_id);
    }

    /// Get the current output
    pub fn get_current_output(&self) -> Option<u32> {
        self.current_output_id
    }

    /// Set the current mode
    pub fn set_current_mode(&mut self, mode_id: u32) {
        self.current_mode = Some(mode_id);
    }

    /// Get the current mode
    pub fn get_current_mode(&self) -> Option<u32> {
        self.current_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crtc_creation() {
        let crtc = KmsCrtc::new(42);
        assert_eq!(crtc.id, 42);
        assert_eq!(crtc.state, CrtcState::Inactive);
        assert!(!crtc.is_active());
    }

    #[test]
    fn test_crtc_activation() {
        let mut crtc = KmsCrtc::new(42);
        crtc.activate();
        assert!(crtc.is_active());
        crtc.deactivate();
        assert!(!crtc.is_active());
    }

    #[test]
    fn test_output_assignment() {
        let mut crtc = KmsCrtc::new(42);
        assert_eq!(crtc.get_current_output(), None);
        crtc.set_current_output(1);
        assert_eq!(crtc.get_current_output(), Some(1));
    }

    #[test]
    fn test_mode_assignment() {
        let mut crtc = KmsCrtc::new(42);
        assert_eq!(crtc.get_current_mode(), None);
        crtc.set_current_mode(10);
        assert_eq!(crtc.get_current_mode(), Some(10));
    }
}
