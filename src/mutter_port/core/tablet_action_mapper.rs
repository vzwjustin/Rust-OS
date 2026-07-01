//! Tablet stylus action mapper ported from GNOME Mutter (src/core/meta-tablet-action-mapper.c).
//!
//! Maps tablet stylus actions (pressure, tilt, buttons) to system actions.
//! Handles stylus pressure sensitivity and button configuration.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-tablet-action-mapper.c
//! Omitted: libwacom integration, ClutterInputDevice handling, X11/Wayland pressure event processing,
//! GObject class machinery, dconf/gsettings configuration

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Identifier for a tablet device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TabletDeviceId(pub u32);

/// Tablet stylus action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StylusAction {
    /// Primary action (e.g., draw/click).
    Primary,
    /// Secondary action (e.g., erase).
    Secondary,
    /// Tertiary action (e.g., navigate).
    Tertiary,
    /// Custom action.
    Custom,
}

/// Tablet button action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabletButtonAction {
    /// Workspace navigation up.
    WorkspaceUp,
    /// Workspace navigation down.
    WorkspaceDown,
    /// Workspace navigation left.
    WorkspaceLeft,
    /// Workspace navigation right.
    WorkspaceRight,
    /// Toggle window exposure/overview.
    ToggleOverview,
    /// Custom action.
    Custom,
}

/// Pressure level sensitivity configuration.
#[derive(Debug, Clone, Copy)]
pub struct PressureCurve {
    /// Minimum pressure threshold.
    pub min_pressure: f32,
    /// Maximum pressure threshold.
    pub max_pressure: f32,
    /// Pressure curve exponent (>1.0 = nonlinear).
    pub exponent: f32,
}

impl Default for PressureCurve {
    fn default() -> Self {
        PressureCurve {
            min_pressure: 0.0,
            max_pressure: 1.0,
            exponent: 1.0,
        }
    }
}

/// Tablet stylus configuration.
#[derive(Debug, Clone)]
pub struct StylusConfig {
    /// Primary stylus action.
    pub primary_action: StylusAction,
    /// Pressure response curve.
    pub pressure_curve: PressureCurve,
    /// Button mappings for stylus buttons.
    pub button_actions: BTreeMap<u32, TabletButtonAction>,
}

impl Default for StylusConfig {
    fn default() -> Self {
        StylusConfig {
            primary_action: StylusAction::Primary,
            pressure_curve: PressureCurve::default(),
            button_actions: BTreeMap::new(),
        }
    }
}

/// Tablet action mapper for stylus devices.
pub struct TabletActionMapper {
    /// Per-device stylus configurations.
    devices: BTreeMap<TabletDeviceId, StylusConfig>,
}

impl TabletActionMapper {
    /// Create a new tablet action mapper.
    pub fn new() -> Self {
        TabletActionMapper {
            devices: BTreeMap::new(),
        }
    }

    /// Register a tablet device with default configuration.
    pub fn register_device(&mut self, device_id: TabletDeviceId) {
        self.devices.insert(device_id, StylusConfig::default());
    }

    /// Unregister a tablet device.
    pub fn unregister_device(&mut self, device_id: TabletDeviceId) {
        self.devices.remove(&device_id);
    }

    /// Get the configuration for a device.
    pub fn get_config(&self, device_id: TabletDeviceId) -> Option<&StylusConfig> {
        self.devices.get(&device_id)
    }

    /// Get a mutable configuration for a device.
    pub fn get_config_mut(&mut self, device_id: TabletDeviceId) -> Option<&mut StylusConfig> {
        self.devices.get_mut(&device_id)
    }

    /// Set the primary stylus action for a device.
    pub fn set_primary_action(&mut self, device_id: TabletDeviceId, action: StylusAction) {
        if let Some(config) = self.devices.get_mut(&device_id) {
            config.primary_action = action;
        }
    }

    /// Get the primary stylus action for a device.
    pub fn get_primary_action(&self, device_id: TabletDeviceId) -> Option<StylusAction> {
        self.devices.get(&device_id).map(|cfg| cfg.primary_action)
    }

    /// Map a stylus button to an action.
    pub fn map_button(
        &mut self,
        device_id: TabletDeviceId,
        button: u32,
        action: TabletButtonAction,
    ) {
        if let Some(config) = self.devices.get_mut(&device_id) {
            config.button_actions.insert(button, action);
        }
    }

    /// Get the action for a stylus button.
    pub fn get_button_action(
        &self,
        device_id: TabletDeviceId,
        button: u32,
    ) -> Option<TabletButtonAction> {
        self.devices
            .get(&device_id)
            .and_then(|cfg| cfg.button_actions.get(&button).copied())
    }

    /// Set the pressure curve for a device.
    pub fn set_pressure_curve(&mut self, device_id: TabletDeviceId, curve: PressureCurve) {
        if let Some(config) = self.devices.get_mut(&device_id) {
            config.pressure_curve = curve;
        }
    }

    /// Get the pressure curve for a device.
    pub fn get_pressure_curve(&self, device_id: TabletDeviceId) -> Option<PressureCurve> {
        self.devices.get(&device_id).map(|cfg| cfg.pressure_curve)
    }

    /// Map pressure value through the device's pressure curve.
    /// Input pressure should be in [0.0, 1.0].
    pub fn map_pressure(&self, device_id: TabletDeviceId, input_pressure: f32) -> f32 {
        if let Some(curve) = self.get_pressure_curve(device_id) {
            // Clamp input
            let clamped = input_pressure.max(0.0).min(1.0);
            // Apply curve: scale to [min, max], apply exponent
            let scaled = curve.min_pressure
                + (libm::powf(clamped, curve.exponent)) * (curve.max_pressure - curve.min_pressure);
            scaled.max(0.0).min(1.0)
        } else {
            input_pressure
        }
    }

    /// Get all registered devices.
    pub fn devices(&self) -> Vec<TabletDeviceId> {
        self.devices.keys().copied().collect()
    }
}

impl Default for TabletActionMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapper_creation() {
        let mapper = TabletActionMapper::new();
        assert_eq!(mapper.devices().len(), 0);
    }

    #[test]
    fn test_register_device() {
        let mut mapper = TabletActionMapper::new();
        let dev_id = TabletDeviceId(1);
        mapper.register_device(dev_id);
        assert!(mapper.get_config(dev_id).is_some());
    }

    #[test]
    fn test_set_primary_action() {
        let mut mapper = TabletActionMapper::new();
        let dev_id = TabletDeviceId(1);
        mapper.register_device(dev_id);

        mapper.set_primary_action(dev_id, StylusAction::Secondary);
        assert_eq!(
            mapper.get_primary_action(dev_id),
            Some(StylusAction::Secondary)
        );
    }

    #[test]
    fn test_map_button() {
        let mut mapper = TabletActionMapper::new();
        let dev_id = TabletDeviceId(1);
        mapper.register_device(dev_id);

        mapper.map_button(dev_id, 0, TabletButtonAction::WorkspaceUp);
        assert_eq!(
            mapper.get_button_action(dev_id, 0),
            Some(TabletButtonAction::WorkspaceUp)
        );
    }

    #[test]
    fn test_pressure_curve() {
        let mut mapper = TabletActionMapper::new();
        let dev_id = TabletDeviceId(1);
        mapper.register_device(dev_id);

        let curve = PressureCurve {
            min_pressure: 0.1,
            max_pressure: 0.9,
            exponent: 2.0,
        };
        mapper.set_pressure_curve(dev_id, curve);
        assert_eq!(mapper.get_pressure_curve(dev_id), Some(curve));
    }

    #[test]
    fn test_map_pressure() {
        let mut mapper = TabletActionMapper::new();
        let dev_id = TabletDeviceId(1);
        mapper.register_device(dev_id);

        let curve = PressureCurve {
            min_pressure: 0.0,
            max_pressure: 1.0,
            exponent: 1.0,
        };
        mapper.set_pressure_curve(dev_id, curve);

        // Identity mapping
        let mapped = mapper.map_pressure(dev_id, 0.5);
        assert!((mapped - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_unregister_device() {
        let mut mapper = TabletActionMapper::new();
        let dev_id = TabletDeviceId(1);
        mapper.register_device(dev_id);
        mapper.unregister_device(dev_id);

        assert_eq!(mapper.devices().len(), 0);
    }
}
