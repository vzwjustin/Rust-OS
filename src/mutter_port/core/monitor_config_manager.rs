//! MetaMonitorConfigManager ported from GNOME Mutter's
//! src/core/meta-monitor-config-manager.c
//!
//! MetaMonitorConfigManager manages monitor configurations: it determines
//! the best configuration for the current set of connected monitors, falls
//! back to a default configuration when no stored config exists, and
//! handles laptop lid switch configurations (internal-only, external-only,
//! docked, mirror).
//!
//! In Mutter this works with MetaMonitorConfigStore for persistence and
//! GSettings for the user's monitor switch config preference. Here the
//! config policy logic is preserved; persistence is handled by the config
//! store module.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-monitor-config-manager.c

use alloc::string::String;
use alloc::vec::Vec;

use crate::mutter_port::backends::logical_monitor::{LogicalMonitor, MonitorTransform, MtkRectangle};
use super::monitor::{MetaMonitor, MonitorKind, MonitorMode};
use super::monitor_manager::{LayoutMode, MetaMonitorManager, MonitorSwitchConfig};

/// A configured monitor: which monitor, which mode, and its position.
/// Mirrors MetaMonitorConfig.
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Connector name of the monitor.
    pub connector: String,
    /// Vendor/product/serial for stable identification.
    pub vendor: String,
    pub product: String,
    pub serial: String,
    /// Selected mode ID.
    pub mode_id: u32,
    /// Whether this monitor is enabled.
    pub enabled: bool,
    /// Whether this is the primary monitor.
    pub primary: bool,
    /// Position in the global layout.
    pub x: i32,
    pub y: i32,
    /// Transform (rotation/reflection).
    pub transform: MonitorTransform,
    /// Scale factor.
    pub scale: f32,
}

impl MonitorConfig {
    pub fn new(connector: &str, mode_id: u32) -> Self {
        MonitorConfig {
            connector: String::from(connector),
            vendor: String::new(),
            product: String::new(),
            serial: String::new(),
            mode_id,
            enabled: true,
            primary: false,
            x: 0,
            y: 0,
            transform: MonitorTransform::Normal,
            scale: 1.0,
        }
    }
}

/// A complete monitor configuration: the set of configured monitors and
/// the layout mode. Mirrors MetaMonitorsConfig.
#[derive(Debug, Clone)]
pub struct MonitorsConfig {
    /// Unique config id.
    pub id: u32,
    /// Per-monitor configurations.
    pub monitor_configs: Vec<MonitorConfig>,
    /// Layout mode (physical or logical).
    pub layout_mode: LayoutMode,
    /// Whether this is a linear (left-to-right) layout.
    pub linear: bool,
    /// Source of this config.
    pub source: ConfigSource,
}

/// Where a configuration came from. Mirrors MetaMonitorsConfigFlag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// Generated from the current hardware state.
    Initial,
    /// Loaded from the persistent config store.
    Stored,
    /// User-requested change.
    User,
    /// Lid switch driven (temporary).
    Temporary,
}

/// The monitor config manager. Mirrors MetaMonitorConfigManager.
#[derive(Debug)]
pub struct MetaMonitorConfigManager {
    /// The current configuration, if any.
    current_config: Option<MonitorsConfig>,
    /// The previous configuration (for undoing temporary changes).
    previous_config: Option<MonitorsConfig>,
    /// The stored (persistent) configuration for the current monitor set.
    stored_config: Option<MonitorsConfig>,
    /// Whether the current config is temporary (lid switch).
    is_current_temporary: bool,
    /// Next config id.
    next_config_id: u32,
}

impl MetaMonitorConfigManager {
    /// Create a new config manager. Mirrors meta_monitor_config_manager_new().
    pub fn new() -> Self {
        MetaMonitorConfigManager {
            current_config: None,
            previous_config: None,
            stored_config: None,
            is_current_temporary: false,
            next_config_id: 1,
        }
    }

    // ── Config creation ───────────────────────────────────────────────

    /// Create a default linear configuration for the given monitors.
    /// Mirrors meta_monitor_config_manager_create_default().
    ///
    /// Places all enabled monitors left-to-right at their preferred mode
    /// with scale 1.0. The first monitor is primary.
    pub fn create_default_linear(&mut self, monitors: &[&MetaMonitor]) -> MonitorsConfig {
        let mut configs = Vec::new();
        let mut x_offset = 0i32;

        for (i, monitor) in monitors.iter().enumerate() {
            // Skip presentation monitors in default layout.
            if monitor.is_presentation() {
                continue;
            }

            // Find preferred mode.
            let mode_id = monitor
                .preferred_mode()
                .or_else(|| monitor.modes().first())
                .map(|m| m.id)
                .unwrap_or(0);

            if mode_id == 0 {
                continue;
            }

            let (w, _) = monitor.dimensions();

            let mut config = MonitorConfig::new(monitor.main_connector(), mode_id);
            config.vendor = String::from(monitor.vendor());
            config.product = String::from(monitor.product());
            config.serial = String::from(monitor.serial());
            config.x = x_offset;
            config.y = 0;
            config.primary = i == 0;
            config.scale = 1.0;

            x_offset += w as i32;
            configs.push(config);
        }

        MonitorsConfig {
            id: self.next_config_id(),
            monitor_configs: configs,
            layout_mode: LayoutMode::Physical,
            linear: true,
            source: ConfigSource::Initial,
        }
    }

    /// Create a mirrored configuration: all monitors show the same content
    /// at the lowest common resolution. Mirrors
    /// meta_monitor_config_manager_create_for_switch_config() with Mirror.
    pub fn create_mirrored(&mut self, monitors: &[&MetaMonitor]) -> MonitorsConfig {
        let mut configs = Vec::new();

        // Find the lowest common resolution.
        let mut min_w = u32::MAX;
        let mut min_h = u32::MAX;
        for monitor in monitors {
            if let Some(mode) = monitor.preferred_mode().or_else(|| monitor.modes().first()) {
                min_w = min_w.min(mode.width);
                min_h = min_h.min(mode.height);
            }
        }

        if min_w == u32::MAX {
            min_w = 1920;
            min_h = 1080;
        }

        for (i, monitor) in monitors.iter().enumerate() {
            // Find the mode closest to the common resolution.
            let mode_id = monitor
                .modes()
                .iter()
                .min_by_key(|m| {
                    (m.width as i64 - min_w as i64).abs() + (m.height as i64 - min_h as i64).abs()
                })
                .map(|m| m.id)
                .unwrap_or(0);

            if mode_id == 0 {
                continue;
            }

            let mut config = MonitorConfig::new(monitor.main_connector(), mode_id);
            config.vendor = String::from(monitor.vendor());
            config.product = String::from(monitor.product());
            config.serial = String::from(monitor.serial());
            config.x = 0;
            config.y = 0;
            config.primary = i == 0;
            config.scale = 1.0;
            configs.push(config);
        }

        MonitorsConfig {
            id: self.next_config_id(),
            monitor_configs: configs,
            layout_mode: LayoutMode::Physical,
            linear: false,
            source: ConfigSource::Temporary,
        }
    }

    /// Create a configuration for a lid switch scenario. Mirrors
    /// meta_monitor_config_manager_create_for_switch_config().
    pub fn create_for_switch_config(
        &mut self,
        monitors: &[&MetaMonitor],
        switch_config: MonitorSwitchConfig,
    ) -> Option<MonitorsConfig> {
        match switch_config {
            MonitorSwitchConfig::ExternalOnly => {
                // Only external monitors, disable builtin.
                let external: Vec<&MetaMonitor> = monitors
                    .iter()
                    .copied()
                    .filter(|m| !m.is_builtin())
                    .collect();
                if external.is_empty() {
                    return None;
                }
                Some(self.create_default_linear(&external))
            }
            MonitorSwitchConfig::InternalOnly => {
                // Only builtin monitor.
                let internal: Vec<&MetaMonitor> = monitors
                    .iter()
                    .copied()
                    .filter(|m| m.is_builtin())
                    .collect();
                if internal.is_empty() {
                    return None;
                }
                Some(self.create_default_linear(&internal))
            }
            MonitorSwitchConfig::Docked => {
                // External monitors on, internal off.
                let external: Vec<&MetaMonitor> = monitors
                    .iter()
                    .copied()
                    .filter(|m| !m.is_builtin())
                    .collect();
                if external.is_empty() {
                    return None;
                }
                Some(self.create_default_linear(&external))
            }
            MonitorSwitchConfig::Mirror => Some(self.create_mirrored(monitors)),
            MonitorSwitchConfig::None => {
                // Use the default linear layout.
                Some(self.create_default_linear(monitors))
            }
        }
    }

    // ── Config management ─────────────────────────────────────────────

    /// Set the current configuration. Mirrors
    /// meta_monitor_config_manager_set_current().
    pub fn set_current(&mut self, config: MonitorsConfig) {
        self.is_current_temporary = config.source == ConfigSource::Temporary;
        self.previous_config = self.current_config.take();
        self.current_config = Some(config);
    }

    /// Get the current configuration.
    pub fn get_current(&self) -> Option<&MonitorsConfig> {
        self.current_config.as_ref()
    }

    /// Whether the current config is temporary (lid switch).
    pub fn is_current_temporary(&self) -> bool {
        self.is_current_temporary
    }

    /// Pop the current config and restore the previous one. Mirrors
    /// meta_monitor_config_manager_pop_current().
    pub fn pop_current(&mut self) -> Option<MonitorsConfig> {
        let current = self.current_config.take();
        self.current_config = self.previous_config.take();
        self.is_current_temporary = false;
        current
    }

    /// Set the stored configuration. Mirrors
    /// meta_monitor_config_manager_set_store().
    pub fn set_stored(&mut self, config: MonitorsConfig) {
        self.stored_config = Some(config);
    }

    /// Get the stored configuration.
    pub fn get_stored(&self) -> Option<&MonitorsConfig> {
        self.stored_config.as_ref()
    }

    /// Whether a stored config exists for the current monitor set.
    pub fn has_stored(&self) -> bool {
        self.stored_config.is_some()
    }

    /// Clear the stored configuration.
    pub fn clear_stored(&mut self) {
        self.stored_config = None;
    }

    // ── Application ───────────────────────────────────────────────────

    /// Apply a configuration to the monitor manager. Mirrors
    /// meta_monitor_manager_apply_monitors_config().
    ///
    /// Creates LogicalMonitor entries from the config and sets them
    /// on the monitor manager.
    pub fn apply_config(
        &self,
        config: &MonitorsConfig,
        monitor_manager: &mut MetaMonitorManager,
    ) -> Result<(), &'static str> {
        let mut logical_monitors = Vec::new();

        for (i, mc) in config.monitor_configs.iter().enumerate() {
            if !mc.enabled {
                continue;
            }

            let logical = LogicalMonitor::new(
                i as i32,
                mc.scale,
                mc.transform,
                MtkRectangle::new(mc.x, mc.y, 0, 0), // width/height filled by monitor
                Vec::new(),
            );

            logical_monitors.push(logical);
        }

        if logical_monitors.is_empty() {
            return Err("No enabled monitors in config");
        }

        monitor_manager.set_logical_monitors(logical_monitors);

        // Set primary from config.
        if let Some(primary_idx) = config.monitor_configs.iter().position(|c| c.primary) {
            monitor_manager.set_primary_monitor(primary_idx);
        }

        Ok(())
    }

    fn next_config_id(&mut self) -> u32 {
        let id = self.next_config_id;
        self.next_config_id += 1;
        id
    }
}

impl Default for MetaMonitorConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::monitor::{MetaMonitor, MonitorMode, MonitorOutput};
    use super::*;

    fn make_monitor(connector: &str, w: u32, h: u32, builtin: bool) -> MetaMonitor {
        let mut output = MonitorOutput::new(connector);
        output.width_mm = if builtin { 300 } else { 527 };
        output.height_mm = if builtin { 170 } else { 296 };
        let mut monitor = MetaMonitor::new_normal(output);
        let mut mode = MonitorMode::new(1, w, h, 60000);
        mode.is_preferred = true;
        monitor.set_modes(vec![mode]);
        monitor
    }

    #[test]
    fn test_empty_manager() {
        let mgr = MetaMonitorConfigManager::new();
        assert!(mgr.get_current().is_none());
        assert!(!mgr.has_stored());
    }

    #[test]
    fn test_create_default_linear() {
        let mut mgr = MetaMonitorConfigManager::new();
        let m1 = make_monitor("DP-1", 1920, 1080, false);
        let m2 = make_monitor("DP-2", 1920, 1080, false);
        let config = mgr.create_default_linear(&[&m1, &m2]);

        assert_eq!(config.monitor_configs.len(), 2);
        assert!(config.linear);
        assert!(config.monitor_configs[0].primary);
        assert!(!config.monitor_configs[1].primary);
        assert_eq!(config.monitor_configs[0].x, 0);
        assert_eq!(config.monitor_configs[1].x, 1920);
    }

    #[test]
    fn test_create_default_linear_single() {
        let mut mgr = MetaMonitorConfigManager::new();
        let m1 = make_monitor("eDP-1", 1920, 1080, true);
        let config = mgr.create_default_linear(&[&m1]);

        assert_eq!(config.monitor_configs.len(), 1);
        assert!(config.monitor_configs[0].primary);
    }

    #[test]
    fn test_create_mirrored() {
        let mut mgr = MetaMonitorConfigManager::new();
        let m1 = make_monitor("eDP-1", 1920, 1080, true);
        let m2 = make_monitor("DP-1", 2560, 1440, false);
        let config = mgr.create_mirrored(&[&m1, &m2]);

        assert_eq!(config.monitor_configs.len(), 2);
        assert!(!config.linear);
        // All at position (0, 0).
        assert_eq!(config.monitor_configs[0].x, 0);
        assert_eq!(config.monitor_configs[1].x, 0);
    }

    #[test]
    fn test_switch_config_external_only() {
        let mut mgr = MetaMonitorConfigManager::new();
        let internal = make_monitor("eDP-1", 1920, 1080, true);
        let external = make_monitor("DP-1", 2560, 1440, false);
        let config = mgr
            .create_for_switch_config(&[&internal, &external], MonitorSwitchConfig::ExternalOnly);

        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.monitor_configs.len(), 1);
        assert_eq!(config.monitor_configs[0].connector, "DP-1");
    }

    #[test]
    fn test_switch_config_internal_only() {
        let mut mgr = MetaMonitorConfigManager::new();
        let internal = make_monitor("eDP-1", 1920, 1080, true);
        let external = make_monitor("DP-1", 2560, 1440, false);
        let config = mgr
            .create_for_switch_config(&[&internal, &external], MonitorSwitchConfig::InternalOnly);

        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.monitor_configs.len(), 1);
        assert_eq!(config.monitor_configs[0].connector, "eDP-1");
    }

    #[test]
    fn test_switch_config_external_only_no_external() {
        let mut mgr = MetaMonitorConfigManager::new();
        let internal = make_monitor("eDP-1", 1920, 1080, true);
        let config = mgr.create_for_switch_config(&[&internal], MonitorSwitchConfig::ExternalOnly);
        assert!(config.is_none());
    }

    #[test]
    fn test_set_and_pop_current() {
        let mut mgr = MetaMonitorConfigManager::new();
        let m1 = make_monitor("DP-1", 1920, 1080, false);
        let config1 = mgr.create_default_linear(&[&m1]);
        mgr.set_current(config1);
        assert!(mgr.get_current().is_some());

        let config2 = mgr.create_mirrored(&[&m1]);
        mgr.set_current(config2);

        // Pop should restore config1.
        let popped = mgr.pop_current();
        assert!(popped.is_some());
        assert_eq!(mgr.get_current().unwrap().linear, true);
    }

    #[test]
    fn test_stored_config() {
        let mut mgr = MetaMonitorConfigManager::new();
        let m1 = make_monitor("DP-1", 1920, 1080, false);
        let config = mgr.create_default_linear(&[&m1]);

        mgr.set_stored(config);
        assert!(mgr.has_stored());
        assert!(mgr.get_stored().is_some());

        mgr.clear_stored();
        assert!(!mgr.has_stored());
    }

    #[test]
    fn test_apply_config() {
        let mut mgr = MetaMonitorConfigManager::new();
        let m1 = make_monitor("DP-1", 1920, 1080, false);
        let config = mgr.create_default_linear(&[&m1]);

        let mut mm = MetaMonitorManager::new();
        assert!(mgr.apply_config(&config, &mut mm).is_ok());
        assert_eq!(mm.get_num_monitors(), 1);
    }

    #[test]
    fn test_apply_empty_config_fails() {
        let config = MonitorsConfig {
            id: 1,
            monitor_configs: Vec::new(),
            layout_mode: LayoutMode::Physical,
            linear: true,
            source: ConfigSource::Initial,
        };
        let mgr = MetaMonitorConfigManager::new();
        let mut mm = MetaMonitorManager::new();
        assert!(mgr.apply_config(&config, &mut mm).is_err());
    }

    #[test]
    fn test_temporary_config_flag() {
        let mut mgr = MetaMonitorConfigManager::new();
        let m1 = make_monitor("DP-1", 1920, 1080, false);
        let config = mgr.create_mirrored(&[&m1]);
        mgr.set_current(config);
        assert!(mgr.is_current_temporary());
    }
}
