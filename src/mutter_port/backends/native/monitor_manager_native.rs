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
}

impl MonitorManagerNative {
    /// Create a new monitor manager
    pub fn new() -> Self {
        MonitorManagerNative {
            kms: Kms::new(),
            gpus: Vec::new(),
            config_mode: ConfigMode::Prefer,
            hotplug_enabled: false,
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

    /// Get total number of monitors
    pub fn get_monitor_count(&self) -> usize {
        self.gpus.iter().map(|gpu| gpu.get_output_count()).sum()
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

    /// Apply display configuration
    /// TODO: Coordinate all GPUs and apply atomic update
    pub fn apply_config(&self) -> Result<(), String> {
        Ok(())
    }

    /// Detect current display configuration
    /// TODO: Query all GPUs and build current configuration
    pub fn detect_config(&mut self) -> Result<(), String> {
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
}
