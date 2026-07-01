//! MetaInputMapper ported from GNOME Mutter's src/core/meta-input-mapper.c
//!
//! MetaInputMapper maps input devices (touchscreens, drawing tablets) to
//! logical monitors. When a touchscreen reports events in its native
//! coordinate space, the mapper determines which monitor the touch should
//! be routed to based on the device's builtin/attached output and the
//! current monitor layout.
//!
//! In Mutter this is a GObject that listens to device-added/removed signals
//! from ClutterInputDevice and monitors-changed from the backend. Here it
//! is a plain struct; callers feed device additions/removals and monitor
//! topology changes explicitly.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-input-mapper.c

use alloc::string::String;
use alloc::vec::Vec;

/// Input device type, mirroring ClutterInputDeviceType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDeviceType {
    /// Touchscreen device.
    Touchscreen,
    /// Drawing tablet / pen device.
    Pen,
    /// Touchpad (not mapped to a specific monitor).
    Touchpad,
    /// Keyboard.
    Keyboard,
    /// Mouse / pointer.
    Mouse,
    /// Other device type.
    Other,
}

/// A logical monitor rectangle for mapping purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl MonitorRect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        MonitorRect {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x >= self.x as f32
            && x < (self.x + self.width) as f32
            && y >= self.y as f32
            && y < (self.y + self.height) as f32
    }
}

/// An input device tracked by the mapper.
#[derive(Debug, Clone)]
pub struct InputDevice {
    /// Unique device identifier.
    pub id: u32,
    /// Device type.
    pub device_type: InputDeviceType,
    /// Device vendor string (e.g. "Wacom").
    pub vendor: String,
    /// Device product string.
    pub product: String,
    /// Whether this is a builtin device (e.g. laptop touchscreen).
    pub is_builtin: bool,
    /// The connector name of the output this device is mapped to, if any.
    /// Mirrors the "Device Node" → output association in Mutter.
    pub mapped_output: Option<String>,
}

impl InputDevice {
    pub fn new(id: u32, device_type: InputDeviceType) -> Self {
        InputDevice {
            id,
            device_type,
            vendor: String::new(),
            product: String::new(),
            is_builtin: false,
            mapped_output: None,
        }
    }

    /// Whether this device should be mapped to a specific monitor.
    /// Only touchscreens and pen tablets need mapping.
    pub fn needs_mapping(&self) -> bool {
        matches!(
            self.device_type,
            InputDeviceType::Touchscreen | InputDeviceType::Pen
        )
    }
}

/// A monitor available for mapping.
#[derive(Debug, Clone)]
pub struct MappingMonitor {
    /// Connector name (e.g. "eDP-1", "DP-1").
    pub connector: String,
    /// Layout rectangle in stage coordinates.
    pub rect: MonitorRect,
    /// Whether this is a builtin panel (laptop screen).
    pub is_builtin: bool,
}

/// A device-to-monitor mapping result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceMapping {
    pub device_id: u32,
    pub connector: String,
}

/// The input mapper. Mirrors MetaInputMapper.
#[derive(Debug)]
pub struct MetaInputMapper {
    devices: Vec<InputDevice>,
    monitors: Vec<MappingMonitor>,
    mappings: Vec<DeviceMapping>,
}

impl MetaInputMapper {
    /// Create a new input mapper. Mirrors meta_input_mapper_new().
    pub fn new() -> Self {
        MetaInputMapper {
            devices: Vec::new(),
            monitors: Vec::new(),
            mappings: Vec::new(),
        }
    }

    // ── Device management ─────────────────────────────────────────────

    /// Add an input device. Mirrors meta_input_mapper_add_device().
    /// Triggers re-mapping if the device needs mapping.
    pub fn add_device(&mut self, device: InputDevice) {
        self.devices.push(device);
        self.remap();
    }

    /// Remove an input device by id. Mirrors meta_input_mapper_remove_device().
    pub fn remove_device(&mut self, device_id: u32) {
        self.devices.retain(|d| d.id != device_id);
        self.mappings.retain(|m| m.device_id != device_id);
    }

    /// Get all tracked devices.
    pub fn get_devices(&self) -> &[InputDevice] {
        &self.devices
    }

    // ── Monitor management ────────────────────────────────────────────

    /// Set the available monitors. Mirrors the monitors-changed handler.
    /// Triggers re-mapping of all devices.
    pub fn set_monitors(&mut self, monitors: Vec<MappingMonitor>) {
        self.monitors = monitors;
        self.remap();
    }

    /// Get all available monitors.
    pub fn get_monitors(&self) -> &[MappingMonitor] {
        &self.monitors
    }

    // ── Mapping queries ───────────────────────────────────────────────

    /// Get the mapping for a device, if any.
    pub fn get_mapping(&self, device_id: u32) -> Option<&DeviceMapping> {
        self.mappings.iter().find(|m| m.device_id == device_id)
    }

    /// Get all current mappings.
    pub fn get_mappings(&self) -> &[DeviceMapping] {
        &self.mappings
    }

    /// Get the monitor rectangle for a mapped device.
    pub fn get_mapped_rect(&self, device_id: u32) -> Option<MonitorRect> {
        let mapping = self.get_mapping(device_id)?;
        let monitor = self
            .monitors
            .iter()
            .find(|m| m.connector == mapping.connector)?;
        Some(monitor.rect)
    }

    // ── Re-mapping logic ──────────────────────────────────────────────

    /// Rebuild all device-to-monitor mappings. Mirrors
    /// meta_input_mapper_remap_devices().
    ///
    /// Strategy (faithful to Mutter):
    /// 1. If a device has a mapped_output connector, use it if present.
    /// 2. If a device is builtin and there's a builtin monitor, map to it.
    /// 3. If there's only one monitor, map all mappable devices to it.
    /// 4. Otherwise, leave unmapped (the caller / settings handle it).
    fn remap(&mut self) {
        self.mappings.clear();

        for device in &self.devices {
            if !device.needs_mapping() {
                continue;
            }

            // Strategy 1: explicit output mapping.
            if let Some(ref connector) = device.mapped_output {
                if self.monitors.iter().any(|m| m.connector == *connector) {
                    self.mappings.push(DeviceMapping {
                        device_id: device.id,
                        connector: connector.clone(),
                    });
                    continue;
                }
            }

            // Strategy 2: builtin device → builtin monitor.
            if device.is_builtin {
                if let Some(builtin) = self.monitors.iter().find(|m| m.is_builtin) {
                    self.mappings.push(DeviceMapping {
                        device_id: device.id,
                        connector: builtin.connector.clone(),
                    });
                    continue;
                }
            }

            // Strategy 3: single monitor → map everything to it.
            if self.monitors.len() == 1 {
                self.mappings.push(DeviceMapping {
                    device_id: device.id,
                    connector: self.monitors[0].connector.clone(),
                });
                continue;
            }

            // Strategy 4: no mapping (left for settings/manual config).
        }
    }

    /// Number of devices that need mapping.
    pub fn mappable_device_count(&self) -> usize {
        self.devices.iter().filter(|d| d.needs_mapping()).count()
    }

    /// Number of devices currently mapped.
    pub fn mapped_count(&self) -> usize {
        self.mappings.len()
    }
}

impl Default for MetaInputMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_touchscreen(id: u32, builtin: bool) -> InputDevice {
        let mut d = InputDevice::new(id, InputDeviceType::Touchscreen);
        d.is_builtin = builtin;
        d
    }

    fn make_monitor(
        connector: &str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        builtin: bool,
    ) -> MappingMonitor {
        MappingMonitor {
            connector: String::from(connector),
            rect: MonitorRect::new(x, y, w, h),
            is_builtin: builtin,
        }
    }

    #[test]
    fn test_empty_mapper() {
        let mapper = MetaInputMapper::new();
        assert_eq!(mapper.get_devices().len(), 0);
        assert_eq!(mapper.get_mappings().len(), 0);
    }

    #[test]
    fn test_builtin_device_to_builtin_monitor() {
        let mut mapper = MetaInputMapper::new();
        mapper.set_monitors(vec![
            make_monitor("eDP-1", 0, 0, 1920, 1080, true),
            make_monitor("DP-1", 1920, 0, 1920, 1080, false),
        ]);
        mapper.add_device(make_touchscreen(1, true));

        let mapping = mapper.get_mapping(1).unwrap();
        assert_eq!(mapping.connector, "eDP-1");
    }

    #[test]
    fn test_single_monitor_maps_all() {
        let mut mapper = MetaInputMapper::new();
        mapper.set_monitors(vec![make_monitor("DP-1", 0, 0, 1920, 1080, false)]);
        mapper.add_device(make_touchscreen(1, false));
        mapper.add_device(make_touchscreen(2, false));

        assert_eq!(mapper.mapped_count(), 2);
        assert_eq!(mapper.get_mapping(1).unwrap().connector, "DP-1");
        assert_eq!(mapper.get_mapping(2).unwrap().connector, "DP-1");
    }

    #[test]
    fn test_explicit_output_mapping() {
        let mut mapper = MetaInputMapper::new();
        mapper.set_monitors(vec![
            make_monitor("eDP-1", 0, 0, 1920, 1080, true),
            make_monitor("DP-1", 1920, 0, 1920, 1080, false),
        ]);

        let mut dev = make_touchscreen(1, true);
        dev.mapped_output = Some(String::from("DP-1"));
        mapper.add_device(dev);

        assert_eq!(mapper.get_mapping(1).unwrap().connector, "DP-1");
    }

    #[test]
    fn test_no_mapping_for_multi_monitor_non_builtin() {
        let mut mapper = MetaInputMapper::new();
        mapper.set_monitors(vec![
            make_monitor("DP-1", 0, 0, 1920, 1080, false),
            make_monitor("DP-2", 1920, 0, 1920, 1080, false),
        ]);
        mapper.add_device(make_touchscreen(1, false));

        assert_eq!(mapper.mapped_count(), 0);
    }

    #[test]
    fn test_remove_device() {
        let mut mapper = MetaInputMapper::new();
        mapper.set_monitors(vec![make_monitor("DP-1", 0, 0, 1920, 1080, false)]);
        mapper.add_device(make_touchscreen(1, false));
        assert_eq!(mapper.mapped_count(), 1);

        mapper.remove_device(1);
        assert_eq!(mapper.get_devices().len(), 0);
        assert_eq!(mapper.mapped_count(), 0);
    }

    #[test]
    fn test_non_mappable_devices_ignored() {
        let mut mapper = MetaInputMapper::new();
        mapper.set_monitors(vec![make_monitor("DP-1", 0, 0, 1920, 1080, false)]);
        mapper.add_device(InputDevice::new(1, InputDeviceType::Keyboard));
        mapper.add_device(InputDevice::new(2, InputDeviceType::Mouse));

        assert_eq!(mapper.mappable_device_count(), 0);
        assert_eq!(mapper.mapped_count(), 0);
    }

    #[test]
    fn test_remap_on_monitor_change() {
        let mut mapper = MetaInputMapper::new();
        mapper.add_device(make_touchscreen(1, true));

        // No monitors → no mapping.
        assert_eq!(mapper.mapped_count(), 0);

        // Add builtin monitor → should map.
        mapper.set_monitors(vec![make_monitor("eDP-1", 0, 0, 1920, 1080, true)]);
        assert_eq!(mapper.mapped_count(), 1);
        assert_eq!(mapper.get_mapping(1).unwrap().connector, "eDP-1");
    }

    #[test]
    fn test_get_mapped_rect() {
        let mut mapper = MetaInputMapper::new();
        mapper.set_monitors(vec![make_monitor("DP-1", 100, 200, 1920, 1080, false)]);
        mapper.add_device(make_touchscreen(1, false));

        let rect = mapper.get_mapped_rect(1).unwrap();
        assert_eq!(rect, MonitorRect::new(100, 200, 1920, 1080));
    }
}
