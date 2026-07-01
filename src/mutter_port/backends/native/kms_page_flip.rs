//! KMS page flip event handling for vblank synchronization.
//!
//! Manages framebuffer swaps and vblank synchronization events.
//! Ported from `meta-kms-page-flip.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Page flip event data
#[derive(Debug, Clone)]
pub struct PageFlipData {
    /// Frame sequence number
    pub sequence: u32,
    /// Timestamp seconds
    pub sec: u32,
    /// Timestamp microseconds
    pub usec: u32,
    /// Whether this is a symbolic (software-only) flip
    pub is_symbolic: bool,
    /// Error if flip failed
    pub error: Option<String>,
}

impl PageFlipData {
    /// Create new page flip data for successful flip
    pub fn new(sequence: u32, sec: u32, usec: u32) -> Self {
        PageFlipData {
            sequence,
            sec,
            usec,
            is_symbolic: false,
            error: None,
        }
    }

    /// Create a symbolic (software-only) flip
    pub fn symbolic(sequence: u32, sec: u32, usec: u32) -> Self {
        PageFlipData {
            sequence,
            sec,
            usec,
            is_symbolic: true,
            error: None,
        }
    }

    /// Create error flip data
    pub fn error(error_msg: String) -> Self {
        PageFlipData {
            sequence: 0,
            sec: 0,
            usec: 0,
            is_symbolic: false,
            error: Some(error_msg),
        }
    }

    /// Get total timestamp in microseconds
    pub fn get_timestamp_us(&self) -> u64 {
        (self.sec as u64) * 1_000_000 + (self.usec as u64)
    }

    /// Check if flip was successful
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}

/// Page flip listener callback
pub trait PageFlipListener: Send + Sync {
    /// Called when page flip completes
    fn on_flip_complete(&self, data: &PageFlipData);
    /// Called when page flip is scheduled
    fn on_flip_scheduled(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_flip_data_creation() {
        let flip = PageFlipData::new(1, 1234, 567890);
        assert_eq!(flip.sequence, 1);
        assert_eq!(flip.sec, 1234);
        assert_eq!(flip.usec, 567890);
        assert!(!flip.is_symbolic);
        assert!(flip.is_success());
    }

    #[test]
    fn test_timestamp() {
        let flip = PageFlipData::new(1, 1, 500000);
        let expected = 1_000_000u64 + 500_000u64;
        assert_eq!(flip.get_timestamp_us(), expected);
    }

    #[test]
    fn test_symbolic_flip() {
        let flip = PageFlipData::symbolic(1, 1234, 567890);
        assert!(flip.is_symbolic);
        assert!(flip.is_success());
    }

    #[test]
    fn test_error_flip() {
        let flip = PageFlipData::error("Permission denied".to_string());
        assert!(!flip.is_success());
        assert_eq!(flip.error, Some("Permission denied".to_string()));
    }
}
