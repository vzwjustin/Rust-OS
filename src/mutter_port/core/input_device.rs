//! MetaInputDevice ported from GNOME Mutter's src/core/meta-input-device.c
//!
//! MetaInputDevice is the compositor-side representation of an input device.
//! It wraps the ClutterInputDevice and adds Mutter-specific state: the
//! device's mapped output, close-on-free flag, and group/keyboard layout
//! tracking.
//!
//! In the kernel, Clutter is not available. This module provides the
//! device data model and state tracking that the seat implementation
//! (seat_impl.rs) and input mapper (input_mapper.rs) use.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-input-device.c

use alloc::string::String;
use alloc::vec::Vec;

use super::seat_impl::DeviceType;

/// Tablet tool type, mirrors ClutterInputDeviceToolType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolType {
    Pen,
    Eraser,
    Brush,
    Pencil,
    Airbrush,
    Mouse,
    Lens,
}

/// Axis type, mirrors ClutterInputAxis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisType {
    X,
    Y,
    Pressure,
    TiltX,
    TiltY,
    Wheel,
    Distance,
    Rotation,
    Slider,
}

/// An axis value reported by a tablet tool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisValue {
    pub axis: AxisType,
    pub value: f64,
}

/// A tablet tool. Mirrors ClutterInputDeviceTool.
#[derive(Debug, Clone)]
pub struct InputDeviceTool {
    /// Tool type.
    pub tool_type: ToolType,
    /// Hardware serial number.
    pub serial: u64,
    /// Hardware tool id.
    pub id: u64,
    /// Pressure curve [0..1]×[0..1] (4 control points).
    pub pressure_curve: [(f32, f32); 4],
    /// Whether the tool has a barrel rotation axis.
    pub has_barrel: bool,
}

impl InputDeviceTool {
    pub fn new(tool_type: ToolType, serial: u64, id: u64) -> Self {
        InputDeviceTool {
            tool_type,
            serial,
            id,
            pressure_curve: [(0.0, 0.0), (0.25, 0.25), (0.75, 0.75), (1.0, 1.0)],
            has_barrel: false,
        }
    }
}

/// The input device. Mirrors MetaInputDevice.
#[derive(Debug)]
pub struct MetaInputDevice {
    /// Unique device id (matches the seat_impl device id).
    pub id: u32,
    /// Device type.
    pub device_type: DeviceType,
    /// Device name.
    pub name: String,
    /// Vendor ID.
    pub vendor_id: u32,
    /// Product ID.
    pub product_id: u32,
    /// Device node path (e.g. /dev/input/event0).
    pub device_node: String,
    /// Dimensions in millimeters.
    pub width_mm: f32,
    pub height_mm: f32,
    /// Number of axes.
    pub n_axes: u32,
    /// Number of buttons.
    pub n_buttons: u32,
    /// Supported axes (for tablets).
    pub axes: Vec<AxisType>,
    /// Associated tablet tool, if any.
    pub tool: Option<InputDeviceTool>,
    /// The connector name of the output this device is mapped to.
    pub mapped_output: Option<String>,
    /// Current keyboard layout group (for keyboards with multiple layouts).
    pub layout_group: u32,
    /// Whether the device is enabled.
    pub enabled: bool,
}

impl MetaInputDevice {
    /// Create a new input device. Mirrors meta_input_device_new().
    pub fn new(id: u32, device_type: DeviceType, name: &str) -> Self {
        MetaInputDevice {
            id,
            device_type,
            name: String::from(name),
            vendor_id: 0,
            product_id: 0,
            device_node: String::new(),
            width_mm: 0.0,
            height_mm: 0.0,
            n_axes: 0,
            n_buttons: 0,
            axes: Vec::new(),
            tool: None,
            mapped_output: None,
            layout_group: 0,
            enabled: true,
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────

    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn mapped_output(&self) -> Option<&str> {
        self.mapped_output.as_deref()
    }

    /// Set the output this device is mapped to. Mirrors
    /// meta_input_device_set_mapped_output().
    pub fn set_mapped_output(&mut self, connector: Option<&str>) {
        self.mapped_output = connector.map(String::from);
    }

    pub fn layout_group(&self) -> u32 {
        self.layout_group
    }

    pub fn set_layout_group(&mut self, group: u32) {
        self.layout_group = group;
    }

    // ── Tablet support ────────────────────────────────────────────────

    /// Whether this device is a tablet (pen/eraser).
    pub fn is_tablet(&self) -> bool {
        matches!(self.device_type, DeviceType::Pen | DeviceType::Eraser)
    }

    /// Set the tablet tool associated with this device.
    pub fn set_tool(&mut self, tool: InputDeviceTool) {
        self.tool = Some(tool);
    }

    pub fn tool(&self) -> Option<&InputDeviceTool> {
        self.tool.as_ref()
    }

    /// Add a supported axis.
    pub fn add_axis(&mut self, axis: AxisType) {
        if !self.axes.contains(&axis) {
            self.axes.push(axis);
            self.n_axes = self.axes.len() as u32;
        }
    }

    /// Whether the device supports the given axis.
    pub fn has_axis(&self, axis: AxisType) -> bool {
        self.axes.contains(&axis)
    }

    // ── Physical dimensions ───────────────────────────────────────────

    pub fn physical_size(&self) -> (f32, f32) {
        (self.width_mm, self.height_mm)
    }

    pub fn set_physical_size(&mut self, width_mm: f32, height_mm: f32) {
        self.width_mm = width_mm;
        self.height_mm = height_mm;
    }

    // ── Device info ───────────────────────────────────────────────────

    pub fn vendor_id(&self) -> u32 {
        self.vendor_id
    }

    pub fn product_id(&self) -> u32 {
        self.product_id
    }

    pub fn set_ids(&mut self, vendor: u32, product: u32) {
        self.vendor_id = vendor;
        self.product_id = product;
    }

    pub fn device_node(&self) -> &str {
        &self.device_node
    }

    pub fn set_device_node(&mut self, node: &str) {
        self.device_node = String::from(node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creation() {
        let dev = MetaInputDevice::new(1, DeviceType::Pointer, "USB Mouse");
        assert_eq!(dev.id, 1);
        assert_eq!(dev.device_type(), DeviceType::Pointer);
        assert_eq!(dev.name(), "USB Mouse");
        assert!(dev.is_enabled());
    }

    #[test]
    fn test_mapped_output() {
        let mut dev = MetaInputDevice::new(1, DeviceType::Touchscreen, "Touch");
        assert!(dev.mapped_output().is_none());

        dev.set_mapped_output(Some("eDP-1"));
        assert_eq!(dev.mapped_output(), Some("eDP-1"));

        dev.set_mapped_output(None);
        assert!(dev.mapped_output().is_none());
    }

    #[test]
    fn test_tablet_tool() {
        let mut dev = MetaInputDevice::new(1, DeviceType::Pen, "Wacom Pen");
        assert!(dev.is_tablet());
        assert!(dev.tool().is_none());

        let tool = InputDeviceTool::new(ToolType::Pen, 12345, 67890);
        dev.set_tool(tool);

        let t = dev.tool().unwrap();
        assert_eq!(t.tool_type, ToolType::Pen);
        assert_eq!(t.serial, 12345);
    }

    #[test]
    fn test_axes() {
        let mut dev = MetaInputDevice::new(1, DeviceType::Pen, "Tablet");
        assert!(!dev.has_axis(AxisType::Pressure));

        dev.add_axis(AxisType::Pressure);
        dev.add_axis(AxisType::TiltX);
        dev.add_axis(AxisType::TiltY);

        assert!(dev.has_axis(AxisType::Pressure));
        assert!(dev.has_axis(AxisType::TiltX));
        assert_eq!(dev.n_axes, 3);

        // Adding duplicate axis should not increase count.
        dev.add_axis(AxisType::Pressure);
        assert_eq!(dev.n_axes, 3);
    }

    #[test]
    fn test_layout_group() {
        let mut dev = MetaInputDevice::new(1, DeviceType::Keyboard, "Kbd");
        assert_eq!(dev.layout_group(), 0);
        dev.set_layout_group(1);
        assert_eq!(dev.layout_group(), 1);
    }

    #[test]
    fn test_enable_disable() {
        let mut dev = MetaInputDevice::new(1, DeviceType::Touchpad, "Touchpad");
        assert!(dev.is_enabled());
        dev.set_enabled(false);
        assert!(!dev.is_enabled());
    }

    #[test]
    fn test_device_ids() {
        let mut dev = MetaInputDevice::new(1, DeviceType::Pointer, "Mouse");
        dev.set_ids(0x046d, 0xc52b);
        assert_eq!(dev.vendor_id(), 0x046d);
        assert_eq!(dev.product_id(), 0xc52b);
    }

    #[test]
    fn test_physical_size() {
        let mut dev = MetaInputDevice::new(1, DeviceType::Touchscreen, "Touch");
        dev.set_physical_size(100.0, 60.0);
        assert_eq!(dev.physical_size(), (100.0, 60.0));
    }

    #[test]
    fn test_pressure_curve_default() {
        let tool = InputDeviceTool::new(ToolType::Pen, 0, 0);
        // Default pressure curve should be linear.
        assert_eq!(tool.pressure_curve[0], (0.0, 0.0));
        assert_eq!(tool.pressure_curve[3], (1.0, 1.0));
    }

    #[test]
    fn test_is_tablet() {
        let pen = MetaInputDevice::new(1, DeviceType::Pen, "Pen");
        let eraser = MetaInputDevice::new(2, DeviceType::Eraser, "Eraser");
        let mouse = MetaInputDevice::new(3, DeviceType::Pointer, "Mouse");

        assert!(pen.is_tablet());
        assert!(eraser.is_tablet());
        assert!(!mouse.is_tablet());
    }
}
