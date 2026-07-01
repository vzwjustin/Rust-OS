//! KMS-based output implementation for hardware display connectors.
//!
//! Manages hardware display outputs via the Linux DRM/KMS subsystem. Handles EDID reading,
//! mode detection, and output hotplug. Ported from `meta-output-kms.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::output_native::{ConnectorType, EdidData, OutputNative, PrivacyScreenState};

/// KMS connector handle (opaque reference to kernel connector)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KmsConnectorHandle(u64);

/// DPMS (Display Power Management Signaling) power level.
///
/// Matches the Linux DRM `DRM_MODE_DPMS_*` constants used by upstream
/// Mutter when querying the `"DPMS"` connector property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum DpmsMode {
    /// On — display is fully active.
    On = 0,
    /// Standby — reduced power, quick resume.
    Standby = 1,
    /// Suspend — deeper power saving.
    Suspend = 2,
    /// Off — display powered down.
    Off = 3,
}

impl DpmsMode {
    /// Create a `DpmsMode` from a raw DRM property value.
    pub fn from_raw(value: u32) -> Self {
        match value {
            1 => DpmsMode::Standby,
            2 => DpmsMode::Suspend,
            3 => DpmsMode::Off,
            _ => DpmsMode::On,
        }
    }

    /// Check whether the display is powered on (On/Standby/Suspend).
    pub fn is_powered(&self) -> bool {
        !matches!(self, DpmsMode::Off)
    }
}

/// KMS output implementation for real hardware
#[derive(Debug)]
pub struct OutputKms {
    /// Base native output
    pub native: OutputNative,
    /// Reference to underlying KMS connector
    pub kms_connector: Option<KmsConnectorHandle>,
    /// DRM connector ID (from kernel)
    pub connector_id: Option<u32>,
    /// Whether this output can be cloned to another CRTC
    pub can_clone: bool,
    /// Backlight interface name if available
    pub backlight_interface: Option<String>,
    /// Index of the currently selected mode within the connector's
    /// mode list, or `None` when no mode is set.
    pub mode_index: Option<usize>,
    /// DRM CRTC ID this output is currently bound to, or `None` when
    /// unbound.
    pub crtc_id: Option<u32>,
    /// Current DPMS power level for this output.
    pub dpms_mode: DpmsMode,
}

impl OutputKms {
    /// Create a new KMS output
    pub fn new(id: u64, connector_type: ConnectorType) -> Self {
        OutputKms {
            native: OutputNative::new(id, connector_type),
            kms_connector: None,
            connector_id: None,
            can_clone: false,
            backlight_interface: None,
            mode_index: None,
            crtc_id: None,
            dpms_mode: DpmsMode::Off,
        }
    }

    /// Set the KMS connector handle
    pub fn set_kms_connector(&mut self, handle: KmsConnectorHandle) {
        self.kms_connector = Some(handle);
    }

    /// Get the KMS connector handle
    pub fn get_kms_connector(&self) -> Option<KmsConnectorHandle> {
        self.kms_connector
    }

    /// Set the DRM connector ID
    pub fn set_connector_id(&mut self, id: u32) {
        self.connector_id = Some(id);
    }

    /// Get the DRM connector ID
    pub fn get_connector_id(&self) -> Option<u32> {
        self.connector_id
    }

    /// Set whether this output can be cloned
    pub fn set_can_clone(&mut self, can_clone: bool) {
        self.can_clone = can_clone;
    }

    /// Check if this output can be cloned
    pub fn can_clone_output(&self) -> bool {
        self.can_clone
    }

    /// Set backlight interface name
    pub fn set_backlight_interface(&mut self, interface: String) {
        self.backlight_interface = Some(interface);
    }

    /// Get backlight interface name
    pub fn get_backlight_interface(&self) -> Option<&str> {
        self.backlight_interface.as_deref()
    }

    /// Set the index of the currently selected mode within the
    /// connector's mode list.
    pub fn set_mode_index(&mut self, index: usize) {
        self.mode_index = Some(index);
    }

    /// Get the currently selected mode index, if any.
    pub fn get_mode_index(&self) -> Option<usize> {
        self.mode_index
    }

    /// Set the DRM CRTC ID this output is bound to. Pass `None` to
    /// mark the output as unbound.
    pub fn set_crtc_id(&mut self, crtc_id: Option<u32>) {
        self.crtc_id = crtc_id;
    }

    /// Get the DRM CRTC ID this output is bound to, if any.
    pub fn get_crtc_id(&self) -> Option<u32> {
        self.crtc_id
    }

    /// Set the current DPMS power level.
    pub fn set_dpms_mode(&mut self, mode: DpmsMode) {
        self.dpms_mode = mode;
    }

    /// Get the current DPMS power level.
    pub fn get_dpms_mode(&self) -> DpmsMode {
        self.dpms_mode
    }

    /// Check whether the output is currently powered on.
    pub fn is_powered(&self) -> bool {
        self.dpms_mode.is_powered()
    }

    /// Read EDID from the connector.
    ///
    /// A full implementation would query the DRM subsystem for the
    /// `"EDID"` blob property via `drmModeGetProperty` /
    /// `drmModeGetPropertyBlob`. Here we return the cached EDID that
    /// was populated when the output was probed.
    pub fn read_edid(&self) -> Option<EdidData> {
        self.native.edid.clone()
    }

    /// Get privacy screen state.
    ///
    /// A full implementation would query the KMS `"privacy-screen"`
    /// connector property. Here we return the cached state.
    pub fn get_privacy_screen_state(&self) -> PrivacyScreenState {
        self.native.privacy_screen_state
    }

    /// Check if this output supports synchronization.
    ///
    /// A full implementation would query the DRM `"vrr_capable"` and
    /// related connector properties. Here we report support only when
    /// a connector id has been assigned and the output is bound to a
    /// CRTC, which is the minimum state required for sync negotiation.
    pub fn supports_sync(&self) -> bool {
        self.connector_id.is_some() && self.crtc_id.is_some()
    }

    /// Get sync tolerance in Hz.
    ///
    /// A full implementation would derive this from the connector's
    /// `"vrr_capable"` property and the current mode's refresh rate.
    /// Here we return a conservative default of 2 Hz, matching the
    /// upstream fallback when no kernel data is available.
    pub fn get_sync_tolerance_hz(&self) -> u32 {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kms_output_creation() {
        let output = OutputKms::new(1, ConnectorType::Hdmi);
        assert_eq!(output.native.id, 1);
        assert_eq!(output.native.connector_type, ConnectorType::Hdmi);
        assert_eq!(output.get_kms_connector(), None);
    }

    #[test]
    fn test_kms_connector_handle() {
        let mut output = OutputKms::new(1, ConnectorType::Hdmi);
        let handle = KmsConnectorHandle(42);
        output.set_kms_connector(handle);
        assert_eq!(output.get_kms_connector(), Some(handle));
    }

    #[test]
    fn test_connector_id() {
        let mut output = OutputKms::new(1, ConnectorType::Hdmi);
        assert_eq!(output.get_connector_id(), None);
        output.set_connector_id(100);
        assert_eq!(output.get_connector_id(), Some(100));
    }

    #[test]
    fn test_backlight_interface() {
        let mut output = OutputKms::new(1, ConnectorType::Hdmi);
        assert_eq!(output.get_backlight_interface(), None);
        output.set_backlight_interface("intel_backlight".to_string());
        assert_eq!(output.get_backlight_interface(), Some("intel_backlight"));
    }

    #[test]
    fn test_clone_capability() {
        let mut output = OutputKms::new(1, ConnectorType::Hdmi);
        assert!(!output.can_clone_output());
        output.set_can_clone(true);
        assert!(output.can_clone_output());
    }

    #[test]
    fn test_mode_index_tracking() {
        let mut output = OutputKms::new(1, ConnectorType::Hdmi);
        assert_eq!(output.get_mode_index(), None);
        output.set_mode_index(3);
        assert_eq!(output.get_mode_index(), Some(3));
    }

    #[test]
    fn test_crtc_id_binding() {
        let mut output = OutputKms::new(1, ConnectorType::Hdmi);
        assert_eq!(output.get_crtc_id(), None);
        output.set_crtc_id(Some(42));
        assert_eq!(output.get_crtc_id(), Some(42));
        output.set_crtc_id(None);
        assert_eq!(output.get_crtc_id(), None);
    }

    #[test]
    fn test_dpms_mode_tracking() {
        let mut output = OutputKms::new(1, ConnectorType::Hdmi);
        assert_eq!(output.get_dpms_mode(), DpmsMode::Off);
        assert!(!output.is_powered());
        output.set_dpms_mode(DpmsMode::On);
        assert!(output.is_powered());
        assert_eq!(DpmsMode::from_raw(3), DpmsMode::Off);
        assert_eq!(DpmsMode::from_raw(0), DpmsMode::On);
    }

    #[test]
    fn test_supports_sync_requires_binding() {
        let mut output = OutputKms::new(1, ConnectorType::Hdmi);
        assert!(!output.supports_sync());
        output.set_connector_id(10);
        assert!(!output.supports_sync());
        output.set_crtc_id(Some(20));
        assert!(output.supports_sync());
    }
}
