//! Tool (eraser/brush) action mapper ported from GNOME Mutter (src/core/meta-tool-action-mapper.c).
//!
//! Maps tablet tool actions (eraser, brush) to system actions and stylus behaviors.
//! Handles tool-specific configuration separate from device-level settings.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-tool-action-mapper.c
//! Omitted: libwacom integration, ClutterInputDevice event handling, X11/Wayland tool event processing,
//! GObject class machinery, dconf/gsettings configuration

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Types of tablet tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolType {
    /// Pen/stylus for drawing/input.
    Pen,
    /// Eraser tool.
    Eraser,
    /// Cursor/pointer tool.
    Cursor,
    /// Brush tool.
    Brush,
}

/// Identifier for a tablet tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ToolId(pub u32);

/// Action for a tablet tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAction {
    /// Draw with the tool.
    Draw,
    /// Erase (for eraser tool).
    Erase,
    /// Scroll action.
    Scroll,
    /// Custom action.
    Custom,
}

/// Tool-specific configuration.
#[derive(Debug, Clone, Copy)]
pub struct ToolConfig {
    /// Type of this tool.
    pub tool_type: ToolType,
    /// Primary action for this tool.
    pub action: ToolAction,
    /// Whether to show cursor when tool is active.
    pub show_cursor: bool,
    /// Pressure sensitivity multiplier.
    pub pressure_sensitivity: f32,
}

impl Default for ToolConfig {
    fn default() -> Self {
        ToolConfig {
            tool_type: ToolType::Pen,
            action: ToolAction::Draw,
            show_cursor: true,
            pressure_sensitivity: 1.0,
        }
    }
}

/// Tool action mapper for tablet tools.
pub struct ToolActionMapper {
    /// Per-tool configurations.
    tools: BTreeMap<ToolId, ToolConfig>,
}

impl ToolActionMapper {
    /// Create a new tool action mapper.
    pub fn new() -> Self {
        ToolActionMapper {
            tools: BTreeMap::new(),
        }
    }

    /// Register a tablet tool with configuration.
    pub fn register_tool(&mut self, tool_id: ToolId, config: ToolConfig) {
        self.tools.insert(tool_id, config);
    }

    /// Register a tool with default configuration.
    pub fn register_tool_default(&mut self, tool_id: ToolId, tool_type: ToolType) {
        let config = ToolConfig {
            tool_type,
            ..Default::default()
        };
        self.tools.insert(tool_id, config);
    }

    /// Unregister a tool.
    pub fn unregister_tool(&mut self, tool_id: ToolId) {
        self.tools.remove(&tool_id);
    }

    /// Get configuration for a tool.
    pub fn get_config(&self, tool_id: ToolId) -> Option<ToolConfig> {
        self.tools.get(&tool_id).copied()
    }

    /// Get mutable configuration for a tool.
    pub fn get_config_mut(&mut self, tool_id: ToolId) -> Option<&mut ToolConfig> {
        self.tools.get_mut(&tool_id)
    }

    /// Set the action for a tool.
    pub fn set_action(&mut self, tool_id: ToolId, action: ToolAction) {
        if let Some(config) = self.tools.get_mut(&tool_id) {
            config.action = action;
        }
    }

    /// Get the action for a tool.
    pub fn get_action(&self, tool_id: ToolId) -> Option<ToolAction> {
        self.tools.get(&tool_id).map(|c| c.action)
    }

    /// Set cursor visibility for a tool.
    pub fn set_show_cursor(&mut self, tool_id: ToolId, show: bool) {
        if let Some(config) = self.tools.get_mut(&tool_id) {
            config.show_cursor = show;
        }
    }

    /// Check if cursor is shown for a tool.
    pub fn should_show_cursor(&self, tool_id: ToolId) -> Option<bool> {
        self.tools.get(&tool_id).map(|c| c.show_cursor)
    }

    /// Set pressure sensitivity for a tool.
    pub fn set_pressure_sensitivity(&mut self, tool_id: ToolId, sensitivity: f32) {
        if let Some(config) = self.tools.get_mut(&tool_id) {
            config.pressure_sensitivity = sensitivity.max(0.1).min(10.0);
        }
    }

    /// Get pressure sensitivity for a tool.
    pub fn get_pressure_sensitivity(&self, tool_id: ToolId) -> Option<f32> {
        self.tools.get(&tool_id).map(|c| c.pressure_sensitivity)
    }

    /// Scale pressure value by tool sensitivity.
    /// Input pressure should be in [0.0, 1.0].
    pub fn scale_pressure(&self, tool_id: ToolId, pressure: f32) -> f32 {
        if let Some(sensitivity) = self.get_pressure_sensitivity(tool_id) {
            (pressure * sensitivity).min(1.0)
        } else {
            pressure
        }
    }

    /// Get tool type for a registered tool.
    pub fn get_tool_type(&self, tool_id: ToolId) -> Option<ToolType> {
        self.tools.get(&tool_id).map(|c| c.tool_type)
    }

    /// Get all registered tools of a specific type.
    pub fn tools_of_type(&self, tool_type: ToolType) -> Vec<ToolId> {
        self.tools
            .iter()
            .filter(|(_, cfg)| cfg.tool_type == tool_type)
            .map(|(&id, _)| id)
            .collect()
    }

    /// Get all registered tools.
    pub fn all_tools(&self) -> Vec<ToolId> {
        self.tools.keys().copied().collect()
    }

    /// Clear all tool registrations.
    pub fn clear(&mut self) {
        self.tools.clear();
    }
}

impl Default for ToolActionMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapper_creation() {
        let mapper = ToolActionMapper::new();
        assert_eq!(mapper.all_tools().len(), 0);
    }

    #[test]
    fn test_register_tool() {
        let mut mapper = ToolActionMapper::new();
        let tool_id = ToolId(1);
        mapper.register_tool_default(tool_id, ToolType::Pen);

        assert!(mapper.get_config(tool_id).is_some());
        assert_eq!(mapper.get_tool_type(tool_id), Some(ToolType::Pen));
    }

    #[test]
    fn test_set_action() {
        let mut mapper = ToolActionMapper::new();
        let tool_id = ToolId(1);
        mapper.register_tool_default(tool_id, ToolType::Eraser);

        mapper.set_action(tool_id, ToolAction::Erase);
        assert_eq!(mapper.get_action(tool_id), Some(ToolAction::Erase));
    }

    #[test]
    fn test_cursor_visibility() {
        let mut mapper = ToolActionMapper::new();
        let tool_id = ToolId(1);
        mapper.register_tool_default(tool_id, ToolType::Pen);

        mapper.set_show_cursor(tool_id, false);
        assert_eq!(mapper.should_show_cursor(tool_id), Some(false));

        mapper.set_show_cursor(tool_id, true);
        assert_eq!(mapper.should_show_cursor(tool_id), Some(true));
    }

    #[test]
    fn test_pressure_sensitivity() {
        let mut mapper = ToolActionMapper::new();
        let tool_id = ToolId(1);
        mapper.register_tool_default(tool_id, ToolType::Brush);

        mapper.set_pressure_sensitivity(tool_id, 2.0);
        assert_eq!(mapper.get_pressure_sensitivity(tool_id), Some(2.0));
    }

    #[test]
    fn test_scale_pressure() {
        let mut mapper = ToolActionMapper::new();
        let tool_id = ToolId(1);
        mapper.register_tool_default(tool_id, ToolType::Pen);

        mapper.set_pressure_sensitivity(tool_id, 2.0);
        let scaled = mapper.scale_pressure(tool_id, 0.5);
        assert!((scaled - 1.0).abs() < 0.01); // Clamped to 1.0
    }

    #[test]
    fn test_tools_of_type() {
        let mut mapper = ToolActionMapper::new();
        mapper.register_tool_default(ToolId(1), ToolType::Pen);
        mapper.register_tool_default(ToolId(2), ToolType::Pen);
        mapper.register_tool_default(ToolId(3), ToolType::Eraser);

        let pens = mapper.tools_of_type(ToolType::Pen);
        assert_eq!(pens.len(), 2);

        let erasers = mapper.tools_of_type(ToolType::Eraser);
        assert_eq!(erasers.len(), 1);
    }

    #[test]
    fn test_unregister_tool() {
        let mut mapper = ToolActionMapper::new();
        let tool_id = ToolId(1);
        mapper.register_tool_default(tool_id, ToolType::Pen);
        assert_eq!(mapper.all_tools().len(), 1);

        mapper.unregister_tool(tool_id);
        assert_eq!(mapper.all_tools().len(), 0);
    }
}
