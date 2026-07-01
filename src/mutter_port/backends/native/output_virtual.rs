//! Virtual output implementation for headless/nested displays.
//!
//! Represents a virtual display output for use in VMs or nested environments.
//! Ported from `meta-output-virtual.c`.

use super::output_native::{ConnectorType, EdidData, OutputNative};

/// Virtual monitor information
#[derive(Debug, Clone)]
pub struct VirtualMonitorInfo {
    /// Display width in pixels
    pub width: u32,
    /// Display height in pixels
    pub height: u32,
    /// Refresh rate in mHz
    pub refresh_rate: u32,
    /// Preferred scale factor (for HiDPI)
    pub preferred_scale: f32,
}

/// Virtual output implementation
#[derive(Debug)]
pub struct OutputVirtual {
    /// Base native output
    pub native: OutputNative,
    /// Virtual monitor information
    pub monitor_info: VirtualMonitorInfo,
}

impl OutputVirtual {
    /// Virtual output ID bit flag (MSB set)
    const ID_BIT: u64 = 1u64 << 63;

    /// Create a new virtual output
    pub fn new(monitor_info: VirtualMonitorInfo) -> Self {
        let id = Self::ID_BIT; // Virtual outputs use high bit set
        let mut native = OutputNative::new(id, ConnectorType::Virtual);
        native.set_connected(true); // Virtual outputs are always connected

        OutputVirtual {
            native,
            monitor_info,
        }
    }

    /// Create a new virtual output with explicit ID
    pub fn with_id(id: u64, monitor_info: VirtualMonitorInfo) -> Self {
        let virtual_id = Self::ID_BIT | (id & !Self::ID_BIT);
        let mut native = OutputNative::new(virtual_id, ConnectorType::Virtual);
        native.set_connected(true); // Virtual outputs are always connected

        OutputVirtual {
            native,
            monitor_info,
        }
    }

    /// Check if this is a virtual output
    pub fn is_virtual(&self) -> bool {
        (self.native.id & Self::ID_BIT) != 0
    }

    /// Get the virtual ID without the ID_BIT marker
    pub fn virtual_id(&self) -> u64 {
        self.native.id & !Self::ID_BIT
    }

    /// Get display dimensions
    pub fn get_size(&self) -> (u32, u32) {
        (self.monitor_info.width, self.monitor_info.height)
    }

    /// Get refresh rate in mHz
    pub fn get_refresh_rate(&self) -> u32 {
        self.monitor_info.refresh_rate
    }

    /// Get scale factor
    pub fn get_scale(&self) -> f32 {
        self.monitor_info.preferred_scale
    }

    /// Virtual outputs don't support hotplug notifications
    pub fn supports_hotplug_mode_update(&self) -> bool {
        false
    }

    /// Get suggested position (-1 means no suggestion)
    pub fn get_suggested_x(&self) -> i32 {
        -1
    }

    pub fn get_suggested_y(&self) -> i32 {
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_output_creation() {
        let info = VirtualMonitorInfo {
            width: 1920,
            height: 1080,
            refresh_rate: 60000,
            preferred_scale: 1.0,
        };
        let output = OutputVirtual::new(info);
        assert!(output.native.connected);
        assert_eq!(output.native.connector_type, ConnectorType::Virtual);
    }

    #[test]
    fn test_is_virtual() {
        let info = VirtualMonitorInfo {
            width: 1920,
            height: 1080,
            refresh_rate: 60000,
            preferred_scale: 1.0,
        };
        let output = OutputVirtual::new(info);
        assert!(output.is_virtual());
    }

    #[test]
    fn test_get_size() {
        let info = VirtualMonitorInfo {
            width: 1920,
            height: 1080,
            refresh_rate: 60000,
            preferred_scale: 1.0,
        };
        let output = OutputVirtual::new(info);
        let (w, h) = output.get_size();
        assert_eq!(w, 1920);
        assert_eq!(h, 1080);
    }
}
