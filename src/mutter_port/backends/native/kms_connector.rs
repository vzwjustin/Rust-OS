//! KMS connector representation for display interfaces.
//!
//! Represents a physical display connector (HDMI, DP, etc.) in the DRM subsystem.
//! Ported from `meta-kms-connector.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Connector state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorState {
    Connected,
    Disconnected,
    Unknown,
}

/// Connector type enum (subset of DRM connector types)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorKmsType {
    Unknown,
    VGA,
    DVII,
    DVID,
    DVIA,
    Composite,
    SVideo,
    LVDS,
    Component,
    NinePinDIN,
    DisplayPort,
    HDMIA,
    HDMIB,
    TV,
    eDP,
    VIRTUAL,
    DSI,
    DPI,
    Writeback,
}

/// KMS connector object
#[derive(Debug, Clone)]
pub struct KmsConnector {
    /// Connector ID from kernel
    pub id: u32,
    /// Connector type
    pub connector_type: ConnectorKmsType,
    /// Current connection state
    pub state: ConnectorState,
    /// Physical connector index (usually)
    pub index: u32,
    /// EDID binary data if available
    pub edid: Option<Vec<u8>>,
    /// Associated CRTC (if connected)
    pub current_crtc_id: Option<u32>,
    /// Possible CRTC mask
    pub possible_crtcs: u32,
}

impl KmsConnector {
    /// Create a new connector
    pub fn new(id: u32, connector_type: ConnectorKmsType, index: u32) -> Self {
        KmsConnector {
            id,
            connector_type,
            state: ConnectorState::Unknown,
            index,
            edid: None,
            current_crtc_id: None,
            possible_crtcs: 0,
        }
    }

    /// Set connection state
    pub fn set_state(&mut self, state: ConnectorState) {
        self.state = state;
    }

    /// Check if connector is connected
    pub fn is_connected(&self) -> bool {
        self.state == ConnectorState::Connected
    }

    /// Set EDID data
    pub fn set_edid(&mut self, edid: Vec<u8>) {
        self.edid = Some(edid);
    }

    /// Get EDID data
    pub fn get_edid(&self) -> Option<&[u8]> {
        self.edid.as_deref()
    }

    /// Set the current CRTC ID
    pub fn set_current_crtc(&mut self, crtc_id: u32) {
        self.current_crtc_id = Some(crtc_id);
    }

    /// Get the current CRTC ID
    pub fn get_current_crtc(&self) -> Option<u32> {
        self.current_crtc_id
    }

    /// Set possible CRTC mask
    pub fn set_possible_crtcs(&mut self, mask: u32) {
        self.possible_crtcs = mask;
    }

    /// Check if this connector can use a specific CRTC
    pub fn can_use_crtc(&self, crtc_id: u32) -> bool {
        (self.possible_crtcs & (1 << crtc_id)) != 0
    }

    /// Get connector name string
    pub fn name(&self) -> String {
        let type_name = match self.connector_type {
            ConnectorKmsType::Unknown => "Unknown",
            ConnectorKmsType::VGA => "VGA",
            ConnectorKmsType::DVII => "DVI-I",
            ConnectorKmsType::DVID => "DVI-D",
            ConnectorKmsType::DVIA => "DVI-A",
            ConnectorKmsType::Composite => "Composite",
            ConnectorKmsType::SVideo => "S-Video",
            ConnectorKmsType::LVDS => "LVDS",
            ConnectorKmsType::Component => "Component",
            ConnectorKmsType::NinePinDIN => "DIN",
            ConnectorKmsType::DisplayPort => "DP",
            ConnectorKmsType::HDMIA => "HDMI-A",
            ConnectorKmsType::HDMIB => "HDMI-B",
            ConnectorKmsType::TV => "TV",
            ConnectorKmsType::eDP => "eDP",
            ConnectorKmsType::VIRTUAL => "Virtual",
            ConnectorKmsType::DSI => "DSI",
            ConnectorKmsType::DPI => "DPI",
            ConnectorKmsType::Writeback => "Writeback",
        };
        format!("{}-{}", type_name, self.index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connector_creation() {
        let connector = KmsConnector::new(1, ConnectorKmsType::HDMIA, 1);
        assert_eq!(connector.id, 1);
        assert_eq!(connector.connector_type, ConnectorKmsType::HDMIA);
        assert_eq!(connector.state, ConnectorState::Unknown);
    }

    #[test]
    fn test_connection_state() {
        let mut connector = KmsConnector::new(1, ConnectorKmsType::HDMIA, 1);
        assert!(!connector.is_connected());
        connector.set_state(ConnectorState::Connected);
        assert!(connector.is_connected());
    }

    #[test]
    fn test_edid_handling() {
        let mut connector = KmsConnector::new(1, ConnectorKmsType::HDMIA, 1);
        assert_eq!(connector.get_edid(), None);
        connector.set_edid(vec![0xFF; 128]);
        assert!(connector.get_edid().is_some());
        assert_eq!(connector.get_edid().unwrap().len(), 128);
    }

    #[test]
    fn test_connector_name() {
        let connector = KmsConnector::new(1, ConnectorKmsType::HDMIA, 1);
        assert_eq!(connector.name(), "HDMI-A-1");
    }

    #[test]
    fn test_crtc_assignment() {
        let mut connector = KmsConnector::new(1, ConnectorKmsType::HDMIA, 1);
        assert_eq!(connector.get_current_crtc(), None);
        connector.set_current_crtc(0);
        assert_eq!(connector.get_current_crtc(), Some(0));
    }
}
