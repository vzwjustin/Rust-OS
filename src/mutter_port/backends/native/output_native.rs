//! Native output abstraction for display connectors.
//!
//! Represents a physical or virtual display output connector (HDMI, DisplayPort, etc.)
//! Ported from `meta-output-native.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Connector type for various display interfaces
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorType {
    /// Unknown connector type
    Unknown,
    /// HDMI
    Hdmi,
    /// DisplayPort
    DisplayPort,
    /// DVI
    Dvi,
    /// VGA
    Vga,
    /// Composite
    Composite,
    /// S-Video
    SVideo,
    /// Component
    Component,
    /// eDP (embedded DisplayPort, typically laptop panels)
    Edp,
    /// Virtual connector (for nested/headless)
    Virtual,
}

/// Privacy screen state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyScreenState {
    Disabled,
    Enabled,
    Unavailable,
}

/// EDID (Extended Display Identification Data) for monitor capabilities
#[derive(Debug, Clone)]
pub struct EdidData {
    /// Raw EDID bytes
    pub data: Vec<u8>,
}

impl EdidData {
    /// Parse EDID data
    pub fn new(data: Vec<u8>) -> Self {
        EdidData { data }
    }

    /// Check if EDID data is valid (basic validation)
    pub fn is_valid(&self) -> bool {
        self.data.len() >= 128 && self.data[0] == 0x00 && self.data[1] == 0xFF
    }
}

/// Abstract base for native outputs
#[derive(Debug)]
pub struct OutputNative {
    /// Unique output identifier
    pub id: u64,
    /// Connector type
    pub connector_type: ConnectorType,
    /// Whether this output is connected
    pub connected: bool,
    /// EDID data if available
    pub edid: Option<EdidData>,
    /// Privacy screen state
    pub privacy_screen_state: PrivacyScreenState,
}

impl OutputNative {
    /// Create a new native output
    pub fn new(id: u64, connector_type: ConnectorType) -> Self {
        OutputNative {
            id,
            connector_type,
            connected: false,
            edid: None,
            privacy_screen_state: PrivacyScreenState::Unavailable,
        }
    }

    /// Set connection status
    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }

    /// Set EDID data
    pub fn set_edid(&mut self, edid: EdidData) {
        self.edid = Some(edid);
    }

    /// Get EDID data if available
    pub fn get_edid(&self) -> Option<&EdidData> {
        self.edid.as_ref()
    }

    /// Set privacy screen state
    pub fn set_privacy_screen_state(&mut self, state: PrivacyScreenState) {
        self.privacy_screen_state = state;
    }

    /// Virtual method: read EDID (to be overridden by subclasses)
    pub fn read_edid(&self) -> Option<EdidData> {
        // Default: return already-cached EDID
        self.edid.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_creation() {
        let output = OutputNative::new(1, ConnectorType::Hdmi);
        assert_eq!(output.id, 1);
        assert_eq!(output.connector_type, ConnectorType::Hdmi);
        assert!(!output.connected);
    }

    #[test]
    fn test_connected_state() {
        let mut output = OutputNative::new(1, ConnectorType::Hdmi);
        assert!(!output.connected);
        output.set_connected(true);
        assert!(output.connected);
    }

    #[test]
    fn test_edid_handling() {
        let mut output = OutputNative::new(1, ConnectorType::Hdmi);
        assert_eq!(output.get_edid(), None);

        let edid = EdidData::new(vec![0xFF; 128]);
        output.set_edid(edid);
        assert!(output.get_edid().is_some());
    }
}
