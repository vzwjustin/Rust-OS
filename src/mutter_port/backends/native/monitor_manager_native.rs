//! Native monitor manager coordinating hardware displays.
//!
//! High-level display management coordinating between multiple outputs and CRTCs.
//! Ported from `meta-monitor-manager-native.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::gpu_kms::GpuKms;
use super::kms::Kms;

/// Monitor configuration mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigMode {
    /// Use existing hardware configuration
    Current,
    /// Prefer-maximum configuration
    Prefer,
    /// Hotplug detection enabled
    Hotplug,
}

/// Native monitor manager
#[derive(Debug)]
pub struct MonitorManagerNative {
    /// KMS subsystem
    pub kms: Kms,
    /// Available GPUs
    pub gpus: Vec<GpuKms>,
    /// Configuration mode
    pub config_mode: ConfigMode,
    /// Whether hotplug detection is enabled
    pub hotplug_enabled: bool,
    /// Cached total number of monitors across all GPUs.
    pub monitor_count: usize,
    /// Index of the primary monitor within the monitor list, or `None`
    /// when no primary monitor has been designated.
    pub primary_index: Option<usize>,
    /// Whether a display configuration has been successfully applied.
    pub config_applied: bool,
}

impl MonitorManagerNative {
    /// Create a new monitor manager
    pub fn new() -> Self {
        MonitorManagerNative {
            kms: Kms::new(),
            gpus: Vec::new(),
            config_mode: ConfigMode::Prefer,
            hotplug_enabled: false,
            monitor_count: 0,
            primary_index: None,
            config_applied: false,
        }
    }

    /// Initialize the monitor manager
    pub fn initialize(&mut self) -> Result<(), String> {
        self.kms.initialize()?;
        self.kms.discover_resources()?;
        Ok(())
    }

    /// Add a GPU to management
    pub fn add_gpu(&mut self, gpu: GpuKms) {
        self.gpus.push(gpu);
    }

    /// Get number of GPUs
    pub fn get_gpu_count(&self) -> usize {
        self.gpus.len()
    }

    /// Get GPU by index
    pub fn get_gpu(&self, index: usize) -> Option<&GpuKms> {
        self.gpus.get(index)
    }

    /// Get mutable GPU by index
    pub fn get_gpu_mut(&mut self, index: usize) -> Option<&mut GpuKms> {
        self.gpus.get_mut(index)
    }

    /// Get total number of monitors.
    ///
    /// Returns the cached count updated by `detect_config`. Callers
    /// that need an immediate recount should call `detect_config`
    /// first.
    pub fn get_monitor_count(&self) -> usize {
        self.monitor_count
    }

    /// Set the index of the primary monitor.
    pub fn set_primary_index(&mut self, index: usize) {
        self.primary_index = Some(index);
    }

    /// Get the index of the primary monitor, if any.
    pub fn get_primary_index(&self) -> Option<usize> {
        self.primary_index
    }

    /// Check whether a configuration has been applied.
    pub fn is_config_applied(&self) -> bool {
        self.config_applied
    }

    /// Set configuration mode
    pub fn set_config_mode(&mut self, mode: ConfigMode) {
        self.config_mode = mode;
    }

    /// Enable/disable hotplug detection
    pub fn set_hotplug_enabled(&mut self, enabled: bool) {
        self.hotplug_enabled = enabled;
    }

    /// Check if hotplug detection is enabled
    pub fn is_hotplug_enabled(&self) -> bool {
        self.hotplug_enabled
    }

    /// Apply display configuration.
    ///
    /// A full implementation would coordinate all GPUs and submit an
    /// atomic update for each affected CRTC/connector. Here we mark
    /// the configuration as applied so callers can observe the state
    /// transition.
    pub fn apply_config(&mut self) -> Result<(), String> {
        self.config_applied = true;
        Ok(())
    }

    /// Detect current display configuration.
    ///
    /// A full implementation would query all GPUs and build the current
    /// configuration from the kernel's reported CRTC/output state. Here
    /// we refresh the cached monitor count from the registered GPUs so
    /// `get_monitor_count` returns an up-to-date value.
    pub fn detect_config(&mut self) -> Result<(), String> {
        self.monitor_count = self.gpus.iter().map(|gpu| gpu.get_output_count()).sum();
        Ok(())
    }
}

impl Default for MonitorManagerNative {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_creation() {
        let manager = MonitorManagerNative::new();
        assert_eq!(manager.get_gpu_count(), 0);
        assert!(!manager.is_hotplug_enabled());
    }

    #[test]
    fn test_manager_initialization() {
        let mut manager = MonitorManagerNative::new();
        let result = manager.initialize();
        assert!(result.is_ok());
    }

    #[test]
    fn test_hotplug_control() {
        let mut manager = MonitorManagerNative::new();
        assert!(!manager.is_hotplug_enabled());
        manager.set_hotplug_enabled(true);
        assert!(manager.is_hotplug_enabled());
    }

    #[test]
    fn test_config_mode() {
        let mut manager = MonitorManagerNative::new();
        manager.set_config_mode(ConfigMode::Hotplug);
        assert_eq!(manager.config_mode, ConfigMode::Hotplug);
    }

    #[test]
    fn test_monitor_count_and_primary() {
        let mut manager = MonitorManagerNative::new();
        assert_eq!(manager.get_monitor_count(), 0);
        assert_eq!(manager.get_primary_index(), None);
        manager.monitor_count = 3;
        manager.set_primary_index(1);
        assert_eq!(manager.get_monitor_count(), 3);
        assert_eq!(manager.get_primary_index(), Some(1));
    }

    #[test]
    fn test_apply_and_detect_config() {
        let mut manager = MonitorManagerNative::new();
        assert!(!manager.is_config_applied());
        manager.apply_config().unwrap();
        assert!(manager.is_config_applied());
        manager.detect_config().unwrap();
        assert_eq!(manager.get_monitor_count(), 0);
    }
}
