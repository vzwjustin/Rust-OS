//! Port of GNOME mutter's `clutter/clutter-input-device-tool.{c,h}`.
//!
//! `ClutterInputDeviceTool` represents a stylus, pen, or tablet tool with
//! a unique serial number. It holds tool metadata (type, hardware ID, axis
//! capabilities) for tablet input tracking across sessions.

use core::fmt;

/// `ClutterInputDeviceToolType` (clutter-enums.h) — enum for tablet tool types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum InputDeviceToolType {
    /// `CLUTTER_INPUT_DEVICE_TOOL_NONE`.
    #[default]
    None = 0,
    /// `CLUTTER_INPUT_DEVICE_TOOL_PEN`.
    Pen = 1,
    /// `CLUTTER_INPUT_DEVICE_TOOL_ERASER`.
    Eraser = 2,
    /// `CLUTTER_INPUT_DEVICE_TOOL_BRUSH`.
    Brush = 3,
    /// `CLUTTER_INPUT_DEVICE_TOOL_PENCIL`.
    Pencil = 4,
    /// `CLUTTER_INPUT_DEVICE_TOOL_AIRBRUSH`.
    Airbrush = 5,
    /// `CLUTTER_INPUT_DEVICE_TOOL_FINGER`.
    Finger = 6,
    /// `CLUTTER_INPUT_DEVICE_TOOL_LENS`.
    Lens = 7,
}

impl fmt::Display for InputDeviceToolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Pen => write!(f, "pen"),
            Self::Eraser => write!(f, "eraser"),
            Self::Brush => write!(f, "brush"),
            Self::Pencil => write!(f, "pencil"),
            Self::Airbrush => write!(f, "airbrush"),
            Self::Finger => write!(f, "finger"),
            Self::Lens => write!(f, "lens"),
        }
    }
}

/// `ClutterInputAxisFlags` (clutter-enums.h) — bitfield for tablet axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct InputAxisFlags(pub u32);

impl InputAxisFlags {
    /// `CLUTTER_INPUT_AXIS_FLAG_X`.
    pub const X: Self = Self(1 << 0);
    /// `CLUTTER_INPUT_AXIS_FLAG_Y`.
    pub const Y: Self = Self(1 << 1);
    /// `CLUTTER_INPUT_AXIS_FLAG_PRESSURE`.
    pub const PRESSURE: Self = Self(1 << 2);
    /// `CLUTTER_INPUT_AXIS_FLAG_XTILT`.
    pub const XTILT: Self = Self(1 << 3);
    /// `CLUTTER_INPUT_AXIS_FLAG_YTILT`.
    pub const YTILT: Self = Self(1 << 4);
    /// `CLUTTER_INPUT_AXIS_FLAG_WHEEL`.
    pub const WHEEL: Self = Self(1 << 5);
    /// `CLUTTER_INPUT_AXIS_FLAG_DISTANCE`.
    pub const DISTANCE: Self = Self(1 << 6);
    /// `CLUTTER_INPUT_AXIS_FLAG_ROTATION`.
    pub const ROTATION: Self = Self(1 << 7);
    /// `CLUTTER_INPUT_AXIS_FLAG_SLIDER`.
    pub const SLIDER: Self = Self(1 << 8);
    pub const NONE: Self = Self(0);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Port of `ClutterInputDeviceTool` / `ClutterInputDeviceToolPrivate`.
#[derive(Debug, Clone)]
pub struct InputDeviceTool {
    pub tool_type: InputDeviceToolType,
    pub serial: u64,
    pub id: u64,
    pub axes: InputAxisFlags,
}

impl Default for InputDeviceTool {
    fn default() -> Self {
        InputDeviceTool {
            tool_type: InputDeviceToolType::None,
            serial: 0,
            id: 0,
            axes: InputAxisFlags::NONE,
        }
    }
}

impl InputDeviceTool {
    /// Construct a new tool with the given properties.
    pub fn new(tool_type: InputDeviceToolType, serial: u64, id: u64, axes: InputAxisFlags) -> Self {
        InputDeviceTool {
            tool_type,
            serial,
            id,
            axes,
        }
    }

    /// `clutter_input_device_tool_get_serial` — get the tool's serial number.
    pub fn serial(&self) -> u64 {
        self.serial
    }

    /// `clutter_input_device_tool_get_tool_type` — get the tool type.
    pub fn tool_type(&self) -> InputDeviceToolType {
        self.tool_type
    }

    /// `clutter_input_device_tool_get_id` — get the tool's hardware ID.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// `clutter_input_device_tool_get_axes` — get the axis capabilities.
    pub fn axes(&self) -> InputAxisFlags {
        self.axes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_none_tool() {
        let t = InputDeviceTool::default();
        assert_eq!(t.tool_type(), InputDeviceToolType::None);
        assert_eq!(t.serial(), 0);
        assert_eq!(t.id(), 0);
        assert!(t.axes().is_empty());
    }

    #[test]
    fn new_sets_fields() {
        let axes = InputAxisFlags::X | InputAxisFlags::Y | InputAxisFlags::PRESSURE;
        let t = InputDeviceTool::new(InputDeviceToolType::Pen, 0x12345, 42, axes);
        assert_eq!(t.tool_type(), InputDeviceToolType::Pen);
        assert_eq!(t.serial(), 0x12345);
        assert_eq!(t.id(), 42);
        assert!(t.axes().contains(InputAxisFlags::PRESSURE));
    }

    #[test]
    fn axes_bitfield_contains() {
        let axes = InputAxisFlags::X | InputAxisFlags::PRESSURE;
        assert!(axes.contains(InputAxisFlags::X));
        assert!(axes.contains(InputAxisFlags::PRESSURE));
        assert!(!axes.contains(InputAxisFlags::WHEEL));
    }

    #[test]
    fn tool_type_display() {
        assert_eq!(InputDeviceToolType::Pen.to_string(), "pen");
        assert_eq!(InputDeviceToolType::Eraser.to_string(), "eraser");
    }
}
