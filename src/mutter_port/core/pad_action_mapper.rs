//! Tablet pad action mapper ported from GNOME Mutter (src/core/meta-pad-action-mapper.c).
//!
//! Maps tablet pad actions (buttons, rings, strips) to compositor/window manager operations.
//! Handles device-specific configuration and action dispatch.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-pad-action-mapper.c
//! Omitted: libwacom integration, ClutterInputDevice handling, low-level input event processing,
//! GObject class machinery, dconf/gsettings configuration

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Types of tablet pad features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PadFeatureType {
    /// Physical buttons on the pad.
    Button,
    /// Ring-shaped input area.
    Ring,
    /// Strip-shaped input area.
    Strip,
}

/// Tablet pad action to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadAction {
    /// Switch workspace up.
    WorkspaceUp,
    /// Switch workspace down.
    WorkspaceDown,
    /// Switch workspace left.
    WorkspaceLeft,
    /// Switch workspace right.
    WorkspaceRight,
    /// Custom action (application-defined).
    Custom,
}

/// Configuration for a single pad feature.
#[derive(Debug, Clone)]
pub struct PadFeatureConfig {
    /// Type of this pad feature.
    pub feature_type: PadFeatureType,
    /// Index of the feature (button number, ring number, etc).
    pub feature_index: u32,
    /// Mode group this feature belongs to (for multi-mode pads).
    pub mode_group: u32,
    /// Action to perform when this feature is triggered.
    pub action: PadAction,
}

impl PadFeatureConfig {
    /// Create a new pad feature configuration.
    pub fn new(feature_type: PadFeatureType, feature_index: u32, action: PadAction) -> Self {
        PadFeatureConfig {
            feature_type,
            feature_index,
            mode_group: 0,
            action,
        }
    }
}

/// Identifier for an input device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(pub u32);

/// Tablet pad action mapper.
///
/// Maps physical pad events to logical actions based on device and user configuration.
pub struct PadActionMapper {
    /// Mappings: (device_id, feature_type, feature_index) -> action
    mappings: BTreeMap<(DeviceId, PadFeatureType, u32), PadAction>,
    /// Mode groups per device: device_id -> current_mode
    mode_groups: BTreeMap<DeviceId, u32>,
}

impl PadActionMapper {
    /// Create a new pad action mapper.
    pub fn new() -> Self {
        PadActionMapper {
            mappings: BTreeMap::new(),
            mode_groups: BTreeMap::new(),
        }
    }

    /// Register a device.
    pub fn register_device(&mut self, device_id: DeviceId) {
        self.mode_groups.insert(device_id, 0);
    }

    /// Unregister a device.
    pub fn unregister_device(&mut self, device_id: DeviceId) {
        self.mode_groups.remove(&device_id);
        // Remove all mappings for this device
        self.mappings
            .retain(|&(dev_id, _, _), _| dev_id != device_id);
    }

    /// Add a mapping for a pad feature.
    pub fn add_mapping(
        &mut self,
        device_id: DeviceId,
        feature_type: PadFeatureType,
        feature_index: u32,
        action: PadAction,
    ) {
        self.mappings
            .insert((device_id, feature_type, feature_index), action);
    }

    /// Get the action for a pad feature.
    pub fn get_action(
        &self,
        device_id: DeviceId,
        feature_type: PadFeatureType,
        feature_index: u32,
    ) -> Option<PadAction> {
        self.mappings
            .get(&(device_id, feature_type, feature_index))
            .copied()
    }

    /// Set the current mode group for a device.
    pub fn set_mode_group(&mut self, device_id: DeviceId, mode: u32) {
        self.mode_groups.insert(device_id, mode);
    }

    /// Get the current mode group for a device.
    pub fn get_mode_group(&self, device_id: DeviceId) -> u32 {
        self.mode_groups.get(&device_id).copied().unwrap_or(0)
    }

    /// Get all mappings for a device.
    pub fn get_device_mappings(
        &self,
        device_id: DeviceId,
    ) -> Vec<(PadFeatureType, u32, PadAction)> {
        self.mappings
            .iter()
            .filter(|&(&(dev_id, _, _), _)| dev_id == device_id)
            .map(|(&(_, feature_type, feature_index), &action)| {
                (feature_type, feature_index, action)
            })
            .collect()
    }

    /// Clear all mappings for a device.
    pub fn clear_device_mappings(&mut self, device_id: DeviceId) {
        self.mappings
            .retain(|&(dev_id, _, _), _| dev_id != device_id);
    }
}

impl Default for PadActionMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapper_creation() {
        let mapper = PadActionMapper::new();
        assert_eq!(mapper.mappings.len(), 0);
    }

    #[test]
    fn test_register_device() {
        let mut mapper = PadActionMapper::new();
        let dev_id = DeviceId(1);
        mapper.register_device(dev_id);
        assert_eq!(mapper.get_mode_group(dev_id), 0);
    }

    #[test]
    fn test_add_mapping() {
        let mut mapper = PadActionMapper::new();
        let dev_id = DeviceId(1);
        mapper.register_device(dev_id);

        mapper.add_mapping(dev_id, PadFeatureType::Button, 0, PadAction::WorkspaceUp);

        assert_eq!(
            mapper.get_action(dev_id, PadFeatureType::Button, 0),
            Some(PadAction::WorkspaceUp)
        );
    }

    #[test]
    fn test_mode_group() {
        let mut mapper = PadActionMapper::new();
        let dev_id = DeviceId(1);
        mapper.register_device(dev_id);

        mapper.set_mode_group(dev_id, 1);
        assert_eq!(mapper.get_mode_group(dev_id), 1);

        mapper.set_mode_group(dev_id, 2);
        assert_eq!(mapper.get_mode_group(dev_id), 2);
    }

    #[test]
    fn test_device_mappings() {
        let mut mapper = PadActionMapper::new();
        let dev_id = DeviceId(1);
        mapper.register_device(dev_id);

        mapper.add_mapping(dev_id, PadFeatureType::Button, 0, PadAction::WorkspaceUp);
        mapper.add_mapping(dev_id, PadFeatureType::Button, 1, PadAction::WorkspaceDown);

        let mappings = mapper.get_device_mappings(dev_id);
        assert_eq!(mappings.len(), 2);
    }

    #[test]
    fn test_clear_mappings() {
        let mut mapper = PadActionMapper::new();
        let dev_id = DeviceId(1);
        mapper.register_device(dev_id);

        mapper.add_mapping(dev_id, PadFeatureType::Button, 0, PadAction::WorkspaceUp);
        assert_eq!(mapper.get_device_mappings(dev_id).len(), 1);

        mapper.clear_device_mappings(dev_id);
        assert_eq!(mapper.get_device_mappings(dev_id).len(), 0);
    }

    #[test]
    fn test_unregister_device() {
        let mut mapper = PadActionMapper::new();
        let dev_id = DeviceId(1);
        mapper.register_device(dev_id);
        mapper.add_mapping(dev_id, PadFeatureType::Button, 0, PadAction::WorkspaceUp);

        mapper.unregister_device(dev_id);
        assert_eq!(mapper.get_mode_group(dev_id), 0); // Default value
        assert_eq!(mapper.get_device_mappings(dev_id).len(), 0);
    }
}
