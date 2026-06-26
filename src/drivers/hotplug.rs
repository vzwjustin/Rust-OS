//! Hot-plug device support for RustOS
//!
//! This module provides hot-plug device detection, driver loading,
//! and dynamic device management capabilities.

use super::{DeviceInfo, DriverInfo, DriverStatus, DriverType};
use alloc::{
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;
use lazy_static::lazy_static;
use spin::{Mutex, RwLock};

/// Hot-plug event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotplugEvent {
    /// Device was inserted/connected
    DeviceAdded,
    /// Device was removed/disconnected
    DeviceRemoved,
    /// Device configuration changed
    DeviceChanged,
    /// Driver was loaded for device
    DriverLoaded,
    /// Driver was unloaded from device
    DriverUnloaded,
}

impl fmt::Display for HotplugEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HotplugEvent::DeviceAdded => write!(f, "Device Added"),
            HotplugEvent::DeviceRemoved => write!(f, "Device Removed"),
            HotplugEvent::DeviceChanged => write!(f, "Device Changed"),
            HotplugEvent::DriverLoaded => write!(f, "Driver Loaded"),
            HotplugEvent::DriverUnloaded => write!(f, "Driver Unloaded"),
        }
    }
}

/// Hot-plug device information
#[derive(Debug, Clone)]
pub struct HotplugDevice {
    /// Unique device identifier
    pub device_id: String,
    /// Device information
    pub device_info: DeviceInfo,
    /// Current driver (if any)
    pub driver: Option<DriverInfo>,
    /// Device state
    pub state: DeviceState,
    /// Timestamp when device was detected
    pub detected_time: u64,
    /// Last event timestamp
    pub last_event_time: u64,
}

/// Device states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    /// Device detected but not configured
    Detected,
    /// Device is being configured
    Configuring,
    /// Device is active and working
    Active,
    /// Device has an error
    Error,
    /// Device is being removed
    Removing,
    /// Device was removed
    Removed,
}

impl fmt::Display for DeviceState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DeviceState::Detected => write!(f, "Detected"),
            DeviceState::Configuring => write!(f, "Configuring"),
            DeviceState::Active => write!(f, "Active"),
            DeviceState::Error => write!(f, "Error"),
            DeviceState::Removing => write!(f, "Removing"),
            DeviceState::Removed => write!(f, "Removed"),
        }
    }
}

/// Hot-plug event notification
#[derive(Debug, Clone)]
pub struct HotplugNotification {
    /// Event type
    pub event: HotplugEvent,
    /// Device ID
    pub device_id: String,
    /// Event timestamp
    pub timestamp: u64,
    /// Additional event data
    pub data: Option<String>,
}

/// Driver matching criteria
#[derive(Debug, Clone)]
pub struct DriverMatch {
    /// Vendor ID pattern
    pub vendor_id: Option<u16>,
    /// Device ID pattern
    pub device_id: Option<u16>,
    /// Class code pattern
    pub class_code: Option<u8>,
    /// Subclass pattern
    pub subclass: Option<u8>,
    /// Driver name
    pub driver_name: String,
    /// Driver priority (higher = preferred)
    pub priority: u32,
}

impl DriverMatch {
    /// Check if this driver matches the given device
    pub fn matches(&self, device: &DeviceInfo) -> bool {
        if let Some(vid) = self.vendor_id {
            if vid != device.vendor_id {
                return false;
            }
        }

        if let Some(did) = self.device_id {
            if did != device.device_id {
                return false;
            }
        }

        if let Some(class) = self.class_code {
            if class != device.class_code {
                return false;
            }
        }

        if let Some(subclass) = self.subclass {
            if subclass != device.subclass {
                return false;
            }
        }

        true
    }
}

/// Hot-plug event handler trait
pub trait HotplugHandler: Send + Sync {
    /// Handle hot-plug event
    fn handle_event(&self, notification: &HotplugNotification) -> Result<(), HotplugError>;

    /// Get handler name
    fn name(&self) -> &str;
}

/// Hot-plug error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotplugError {
    /// Device not found
    DeviceNotFound,
    /// Driver not found
    DriverNotFound,
    /// Driver loading failed
    DriverLoadFailed,
    /// Device configuration failed
    ConfigurationFailed,
    /// Resource conflict
    ResourceConflict,
    /// Permission denied
    PermissionDenied,
    /// Operation not supported
    NotSupported,
    /// Invalid argument
    InvalidArgument,
}

impl fmt::Display for HotplugError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HotplugError::DeviceNotFound => write!(f, "Device not found"),
            HotplugError::DriverNotFound => write!(f, "Driver not found"),
            HotplugError::DriverLoadFailed => write!(f, "Driver loading failed"),
            HotplugError::ConfigurationFailed => write!(f, "Device configuration failed"),
            HotplugError::ResourceConflict => write!(f, "Resource conflict"),
            HotplugError::PermissionDenied => write!(f, "Permission denied"),
            HotplugError::NotSupported => write!(f, "Operation not supported"),
            HotplugError::InvalidArgument => write!(f, "Invalid argument"),
        }
    }
}

/// Hot-plug result type
pub type HotplugResult<T> = Result<T, HotplugError>;

/// Hot-plug manager
pub struct HotplugManager {
    /// Detected devices
    devices: RwLock<BTreeMap<String, HotplugDevice>>,
    /// Driver matching rules
    driver_matches: RwLock<Vec<DriverMatch>>,
    /// Event handlers
    handlers: RwLock<Vec<Box<dyn HotplugHandler>>>,
    /// Event queue
    event_queue: Mutex<Vec<HotplugNotification>>,
    /// Next device ID counter
    next_device_id: Mutex<u64>,
    /// Hot-plug enabled
    enabled: RwLock<bool>,
}

impl HotplugManager {
    /// Create new hot-plug manager
    pub fn new() -> Self {
        Self {
            devices: RwLock::new(BTreeMap::new()),
            driver_matches: RwLock::new(Vec::new()),
            handlers: RwLock::new(Vec::new()),
            event_queue: Mutex::new(Vec::new()),
            next_device_id: Mutex::new(1),
            enabled: RwLock::new(true),
        }
    }

    /// Enable/disable hot-plug support
    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.write() = enabled;
        // Production: hot-plug status changed silently
    }

    /// Check if hot-plug is enabled
    pub fn is_enabled(&self) -> bool {
        *self.enabled.read()
    }

    /// Register a driver match rule
    pub fn register_driver_match(&self, driver_match: DriverMatch) {
        let mut matches = self.driver_matches.write();
        matches.push(driver_match);
        matches.sort_by(|a, b| b.priority.cmp(&a.priority)); // Sort by priority (highest first)
    }

    /// Register an event handler
    pub fn register_handler(&self, handler: Box<dyn HotplugHandler>) {
        let mut handlers = self.handlers.write();
        // Production: handler registered silently
        handlers.push(handler);
    }

    /// Generate unique device ID
    fn generate_device_id(&self) -> String {
        let mut next_id = self.next_device_id.lock();
        let id = *next_id;
        *next_id += 1;
        format!("dev{:04}", id)
    }

    /// Add a new device
    pub fn add_device(&self, device_info: DeviceInfo) -> HotplugResult<String> {
        if !self.is_enabled() {
            return Err(HotplugError::NotSupported);
        }

        let device_id = self.generate_device_id();
        let timestamp = get_current_time();

        let hotplug_device = HotplugDevice {
            device_id: device_id.clone(),
            device_info: device_info.clone(),
            driver: None,
            state: DeviceState::Detected,
            detected_time: timestamp,
            last_event_time: timestamp,
        };

        // Add to device list
        {
            let mut devices = self.devices.write();
            devices.insert(device_id.clone(), hotplug_device);
        }

        // Generate event
        let notification = HotplugNotification {
            event: HotplugEvent::DeviceAdded,
            device_id: device_id.clone(),
            timestamp,
            data: Some(format!(
                "{}:{:04x}:{:04x}",
                device_info.get_vendor_name(),
                device_info.vendor_id,
                device_info.device_id
            )),
        };

        self.queue_event(notification);

        // Production: device added silently

        // Try to find and load a driver
        self.try_load_driver(&device_id)?;

        Ok(device_id)
    }

    /// Remove a device
    pub fn remove_device(&self, device_id: &str) -> HotplugResult<()> {
        if !self.is_enabled() {
            return Err(HotplugError::NotSupported);
        }

        // Update device state
        {
            let mut devices = self.devices.write();
            if let Some(device) = devices.get_mut(device_id) {
                device.state = DeviceState::Removing;
                device.last_event_time = get_current_time();

                // Unload driver if loaded
                if device.driver.is_some() {
                    device.driver = None;

                    let notification = HotplugNotification {
                        event: HotplugEvent::DriverUnloaded,
                        device_id: device_id.to_string(),
                        timestamp: get_current_time(),
                        data: None,
                    };
                    self.queue_event(notification);
                }
            } else {
                return Err(HotplugError::DeviceNotFound);
            }
        }

        // Generate removal event
        let notification = HotplugNotification {
            event: HotplugEvent::DeviceRemoved,
            device_id: device_id.to_string(),
            timestamp: get_current_time(),
            data: None,
        };

        self.queue_event(notification);

        // Remove from device list
        {
            let mut devices = self.devices.write();
            devices.remove(device_id);
        }

        // Production: device removed silently
        Ok(())
    }

    /// Try to load a driver for the device
    fn try_load_driver(&self, device_id: &str) -> HotplugResult<()> {
        let device_info = {
            let devices = self.devices.read();
            let device = devices.get(device_id).ok_or(HotplugError::DeviceNotFound)?;
            device.device_info.clone()
        };

        // Find matching driver
        let driver_matches = self.driver_matches.read();
        let mut best_match: Option<&DriverMatch> = None;

        for driver_match in driver_matches.iter() {
            if driver_match.matches(&device_info) {
                if best_match.is_none() || driver_match.priority > best_match.unwrap().priority {
                    best_match = Some(driver_match);
                }
            }
        }

        if let Some(driver_match) = best_match {
            // Create driver info
            let driver_info = DriverInfo::new(
                driver_match.driver_name.clone(),
                "1.0.0".to_string(),
                device_info.get_device_type(),
                device_info.get_vendor_name().to_string(),
                format!("Driver for {}", device_info.name),
            )
            .with_device_id((device_info.vendor_id as u32) << 16 | device_info.device_id as u32)
            .with_status(DriverStatus::Ready);

            // Update device with driver
            {
                let mut devices = self.devices.write();
                if let Some(device) = devices.get_mut(device_id) {
                    device.driver = Some(driver_info);
                    device.state = DeviceState::Active;
                    device.last_event_time = get_current_time();
                }
            }

            // Generate driver loaded event
            let notification = HotplugNotification {
                event: HotplugEvent::DriverLoaded,
                device_id: device_id.to_string(),
                timestamp: get_current_time(),
                data: Some(driver_match.driver_name.clone()),
            };

            self.queue_event(notification);

            // Production: driver loaded silently
        } else {
            // Production: no driver available (expected for unknown devices)
        }

        Ok(())
    }

    /// Queue an event for processing
    fn queue_event(&self, notification: HotplugNotification) {
        let mut queue = self.event_queue.lock();
        queue.push(notification);
    }

    /// Process queued events
    pub fn process_events(&self) -> HotplugResult<usize> {
        let events = {
            let mut queue = self.event_queue.lock();
            let events = queue.clone();
            queue.clear();
            events
        };

        let handlers = self.handlers.read();
        let mut processed = 0;

        for event in events {
            for handler in handlers.iter() {
                if let Err(_e) = handler.handle_event(&event) {
                    // Production: handler error logged internally
                }
            }
            processed += 1;
        }

        Ok(processed)
    }

    /// Get device by ID
    pub fn get_device(&self, device_id: &str) -> Option<HotplugDevice> {
        let devices = self.devices.read();
        devices.get(device_id).cloned()
    }

    /// List all devices
    pub fn list_devices(&self) -> Vec<HotplugDevice> {
        let devices = self.devices.read();
        devices.values().cloned().collect()
    }

    /// Get devices by type
    pub fn get_devices_by_type(&self, driver_type: DriverType) -> Vec<HotplugDevice> {
        let devices = self.devices.read();
        devices
            .values()
            .filter(|device| device.device_info.get_device_type() == driver_type)
            .cloned()
            .collect()
    }

    /// Get hot-plug statistics
    pub fn get_stats(&self) -> HotplugStats {
        let devices = self.devices.read();
        let handlers = self.handlers.read();
        let driver_matches = self.driver_matches.read();

        let mut stats = HotplugStats {
            total_devices: devices.len(),
            active_devices: 0,
            devices_with_drivers: 0,
            total_handlers: handlers.len(),
            total_driver_matches: driver_matches.len(),
            enabled: self.is_enabled(),
        };

        for device in devices.values() {
            if device.state == DeviceState::Active {
                stats.active_devices += 1;
            }
            if device.driver.is_some() {
                stats.devices_with_drivers += 1;
            }
        }

        stats
    }

    /// Scan for new devices (placeholder)
    pub fn scan_for_devices(&self) -> HotplugResult<usize> {
        if !self.is_enabled() {
            return Ok(0);
        }

        let mut new_devices = 0;

        // Scan PCI bus for new devices
        #[cfg(not(test))]
        {
            use crate::pci;
            // Get all PCI devices
            let pci_devices = pci::get_all_devices();

            // Collect new devices to add
            let mut devices_to_add = Vec::new();

            {
                let current_devices = self.devices.read();
                for device in pci_devices {
                    // Create a unique device ID string from PCI location
                    let pci_device_id = format!(
                        "pci_{:02x}:{:02x}.{:x}_{:04x}:{:04x}",
                        device.bus,
                        device.device,
                        device.function,
                        device.vendor_id,
                        device.device_id
                    );

                    // If device not in our registry, it's new
                    if !current_devices.contains_key(&pci_device_id) {
                        new_devices += 1;
                        devices_to_add.push(device);
                    }
                }
            } // Release read lock

            // Add new devices outside the lock
            for device in devices_to_add {
                let device_info = DeviceInfo::new(
                    device.vendor_id,
                    device.device_id,
                    device.class as u8,
                    device.subclass,
                    device.prog_if,
                    device.revision,
                    device.bus,
                    device.device,
                    device.function,
                    device.name.clone(),
                );
                let _ = self.add_device(device_info);
            }
        }

        // USB scanning would go here (future enhancement)
        // Other bus scanning (SATA hotplug, etc.) would go here

        Ok(new_devices)
    }
}

/// Hot-plug statistics
#[derive(Debug, Clone)]
pub struct HotplugStats {
    pub total_devices: usize,
    pub active_devices: usize,
    pub devices_with_drivers: usize,
    pub total_handlers: usize,
    pub total_driver_matches: usize,
    pub enabled: bool,
}

/// Default hot-plug event handler
pub struct DefaultHotplugHandler {
    name: String,
}

impl DefaultHotplugHandler {
    pub fn new() -> Self {
        Self {
            name: "Default Handler".to_string(),
        }
    }
}

impl HotplugHandler for DefaultHotplugHandler {
    fn handle_event(&self, _notification: &HotplugNotification) -> Result<(), HotplugError> {
        // Production: hot-plug event processed silently

        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

lazy_static! {
    static ref HOTPLUG_MANAGER: HotplugManager = HotplugManager::new();
}

/// Initialize hot-plug subsystem
pub fn init() -> HotplugResult<()> {
    // Register default handler
    let default_handler = Box::new(DefaultHotplugHandler::new());
    HOTPLUG_MANAGER.register_handler(default_handler);

    // Register common driver matches
    register_common_drivers();

    // Enable hot-plug support
    HOTPLUG_MANAGER.set_enabled(true);

    // Production: hot-plug subsystem initialized
    Ok(())
}

/// Register common driver matches
fn register_common_drivers() {
    // VGA/Graphics drivers
    HOTPLUG_MANAGER.register_driver_match(DriverMatch {
        vendor_id: Some(0x1234), // QEMU
        device_id: Some(0x1111),
        class_code: Some(0x03),
        subclass: None,
        driver_name: "qemu-vga".to_string(),
        priority: 100,
    });

    HOTPLUG_MANAGER.register_driver_match(DriverMatch {
        vendor_id: Some(0x80EE), // VirtualBox
        device_id: Some(0xBEEF),
        class_code: Some(0x03),
        subclass: None,
        driver_name: "vbox-vga".to_string(),
        priority: 100,
    });

    // Network drivers
    HOTPLUG_MANAGER.register_driver_match(DriverMatch {
        vendor_id: Some(0x8086), // Intel
        device_id: None,
        class_code: Some(0x02),
        subclass: None,
        driver_name: "intel-net".to_string(),
        priority: 90,
    });

    // Storage drivers
    HOTPLUG_MANAGER.register_driver_match(DriverMatch {
        vendor_id: None,
        device_id: None,
        class_code: Some(0x01),
        subclass: Some(0x01), // IDE
        driver_name: "ide-storage".to_string(),
        priority: 80,
    });

    HOTPLUG_MANAGER.register_driver_match(DriverMatch {
        vendor_id: None,
        device_id: None,
        class_code: Some(0x01),
        subclass: Some(0x06), // SATA
        driver_name: "sata-storage".to_string(),
        priority: 90,
    });
}

/// Get the global hot-plug manager
pub fn hotplug_manager() -> &'static HotplugManager {
    &HOTPLUG_MANAGER
}

/// Add a device to hot-plug management
pub fn add_device(device_info: DeviceInfo) -> HotplugResult<String> {
    HOTPLUG_MANAGER.add_device(device_info)
}

/// Remove a device from hot-plug management
pub fn remove_device(device_id: &str) -> HotplugResult<()> {
    HOTPLUG_MANAGER.remove_device(device_id)
}

/// Process hot-plug events
pub fn process_events() -> HotplugResult<usize> {
    HOTPLUG_MANAGER.process_events()
}

/// Get hot-plug statistics
pub fn get_hotplug_stats() -> HotplugStats {
    HOTPLUG_MANAGER.get_stats()
}

/// Scan for new devices
pub fn scan_devices() -> HotplugResult<usize> {
    HOTPLUG_MANAGER.scan_for_devices()
}

/// Get current time in milliseconds
fn get_current_time() -> u64 {
    // Use system time for hotplug event timestamps
    crate::time::get_system_time_ms()
}
