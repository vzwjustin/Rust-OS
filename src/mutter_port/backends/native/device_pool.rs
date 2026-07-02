//! Device pool for managing input devices.
//!
//! Manages enumeration and lifecycle of input devices (keyboard, mouse, touchscreen, etc.).
//! Ported from `meta-device-pool.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Input device type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDeviceType {
    Keyboard,
    Mouse,
    Touchpad,
    Touchscreen,
    Tablet,
    Other,
}

/// Input device information
#[derive(Debug, Clone)]
pub struct InputDevice {
    /// Device ID
    pub id: u32,
    /// Device type
    pub device_type: InputDeviceType,
    /// Device name
    pub name: String,
    /// Path to device file
    pub path: String,
}

impl InputDevice {
    /// Create a new input device
    pub fn new(id: u32, device_type: InputDeviceType, name: String, path: String) -> Self {
        InputDevice {
            id,
            device_type,
            name,
            path,
        }
    }
}

/// Pool of input devices
#[derive(Debug)]
pub struct DevicePool {
    /// List of devices
    pub devices: Vec<InputDevice>,
}

impl DevicePool {
    /// Create a new device pool
    pub fn new() -> Self {
        DevicePool {
            devices: Vec::new(),
        }
    }

    /// Add a device to the pool
    pub fn add_device(&mut self, device: InputDevice) {
        self.devices.push(device);
    }

    /// Remove device by ID
    pub fn remove_device(&mut self, id: u32) -> Option<InputDevice> {
        if let Some(pos) = self.devices.iter().position(|d| d.id == id) {
            Some(self.devices.remove(pos))
        } else {
            None
        }
    }

    /// Get device by ID
    pub fn get_device(&self, id: u32) -> Option<&InputDevice> {
        self.devices.iter().find(|d| d.id == id)
    }

    /// Get mutable device by ID
    pub fn get_device_mut(&mut self, id: u32) -> Option<&mut InputDevice> {
        self.devices.iter_mut().find(|d| d.id == id)
    }

    /// Get all keyboard devices
    pub fn get_keyboards(&self) -> Vec<&InputDevice> {
        self.devices
            .iter()
            .filter(|d| d.device_type == InputDeviceType::Keyboard)
            .collect()
    }

    /// Get all pointer devices (mouse, touchpad, touchscreen)
    pub fn get_pointers(&self) -> Vec<&InputDevice> {
        self.devices
            .iter()
            .filter(|d| {
                d.device_type == InputDeviceType::Mouse
                    || d.device_type == InputDeviceType::Touchpad
                    || d.device_type == InputDeviceType::Touchscreen
            })
            .collect()
    }

    /// Scan system for input devices. A full implementation would use
    /// udev or scan /dev/input/ to enumerate connected devices. Without
    /// a filesystem, the device pool remains empty.
    pub fn scan_devices(&mut self) {
        // Device enumeration requires udev or /dev/input/ access.
        // Without a filesystem, no devices are discovered.
    }

    /// Get the total number of devices.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Get all devices of a specific type.
    pub fn get_devices_by_type(&self, device_type: InputDeviceType) -> Vec<&InputDevice> {
        self.devices
            .iter()
            .filter(|d| d.device_type == device_type)
            .collect()
    }

    /// Check if a device with the given ID exists.
    pub fn has_device(&self, id: u32) -> bool {
        self.devices.iter().any(|d| d.id == id)
    }

    /// Clear all devices from the pool.
    pub fn clear(&mut self) {
        self.devices.clear();
    }
}

impl Default for DevicePool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_creation() {
        let pool = DevicePool::new();
        assert_eq!(pool.devices.len(), 0);
    }

    #[test]
    fn test_add_device() {
        let mut pool = DevicePool::new();
        let device = InputDevice::new(
            1,
            InputDeviceType::Keyboard,
            "Keyboard".to_string(),
            "/dev/input/event0".to_string(),
        );
        pool.add_device(device);
        assert_eq!(pool.devices.len(), 1);
    }

    #[test]
    fn test_get_keyboards() {
        let mut pool = DevicePool::new();
        pool.add_device(InputDevice::new(
            1,
            InputDeviceType::Keyboard,
            "Keyboard".to_string(),
            "/dev/input/event0".to_string(),
        ));
        pool.add_device(InputDevice::new(
            2,
            InputDeviceType::Mouse,
            "Mouse".to_string(),
            "/dev/input/event1".to_string(),
        ));

        let keyboards = pool.get_keyboards();
        assert_eq!(keyboards.len(), 1);
        assert_eq!(keyboards[0].id, 1);
    }
}
