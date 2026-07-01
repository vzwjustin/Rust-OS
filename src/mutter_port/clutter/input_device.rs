//! Port of GNOME mutter's `clutter/clutter-input-device.{c,h}` and the
//! input-related enums from `clutter/clutter-enums.h`.
//!
//! `ClutterInputDevice` describes one input device (pointer, keyboard,
//! touchpad, tablet, ...). It's a plain data class in upstream — the
//! interesting behavior (event dispatch, grabs, tool/frame clocks) lives
//! in the backend and `ClutterSeat`, not on the device object itself — so
//! it ports cleanly to a plain struct.
//!
//! # What's ported
//!
//! - The `ClutterInputDevicePrivate` field layout (`device_type`,
//!   `capabilities`, `device_name`, `vendor_id`, `product_id`, `bus_type`,
//!   `node_path`, `n_rings`, `n_strips`, `n_dials`, `n_mode_groups`,
//!   `n_buttons`) as a plain `InputDevice` struct. The `seat` back-pointer
//!   is omitted (no `ClutterSeat` port yet); a future seat port can add
//!   an `Option<SeatId>`.
//! - `clutter_input_device_init` default (`device_type = POINTER`).
//! - `clutter_input_device_constructed`'s capability inference: when
//!   `capabilities` is empty, derive it from `device_type` via
//!   `InputDevice::infer_capabilities` (called from `new`).
//! - The getters: `device_type`/`capabilities`/`device_name`/`vendor_id`/
//!   `product_id`/`bus_type`/`node_path`/`n_rings`/`n_strips`/`n_dials`/
//!   `n_mode_groups`/`n_buttons`.
//! - The `ClutterInputDeviceType` and `ClutterInputCapabilities` enums
//!   from `clutter-enums.h`, with values matching the C numbering
//!   (capabilities is a bitfield; device type is sequential).
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_TYPE_WITH_PRIVATE`, `GParamSpec`
//!   property install/notify, `constructed`/`dispose`/`set_property`/
//!   `get_property`): plain fields + a `new` constructor that runs the
//!   capability inference. The `dispose` `g_clear_pointer` on
//!   `device_name`/`node_path` becomes normal `String`/`Option<String>`
//!   drop.
//! - The `seat` field and `clutter_input_device_get_seat`: no `ClutterSeat`
//!   port yet.
//! - Axis/key/scroll/get-coordinates accessors (`get_axes`/`get_n_axes`/
//!   `get_axis_value`/`get_coords`/`get_key`/`set_key`/...): these query
//!   per-axis metadata and the current pointer position, which require a
//!   backend (evdev/libinput) integration not ported yet. They're left for
//!   the backend-port wave.
//! - `update_from_tool` / `copy_axes` / tool-specific state: tablet-tool
//!   integration, backend-dependent.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use alloc::string::{String, ToString};

/// `ClutterInputDeviceType` (clutter-enums.h). Values match the C
/// numbering (sequential from 0); `NDeviceTypes` is included as the
/// sentinel count for iteration, matching upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum InputDeviceType {
    /// `CLUTTER_POINTER_DEVICE`.
    #[default]
    Pointer = 0,
    /// `CLUTTER_KEYBOARD_DEVICE`.
    Keyboard = 1,
    /// `CLUTTER_EXTENSION_DEVICE`.
    Extension = 2,
    /// `CLUTTER_JOYSTICK_DEVICE`.
    Joystick = 3,
    /// `CLUTTER_TABLET_DEVICE`.
    Tablet = 4,
    /// `CLUTTER_TOUCHPAD_DEVICE`.
    Touchpad = 5,
    /// `CLUTTER_TOUCHSCREEN_DEVICE`.
    Touchscreen = 6,
    /// `CLUTTER_PEN_DEVICE`.
    Pen = 7,
    /// `CLUTTER_ERASER_DEVICE`.
    Eraser = 8,
    /// `CLUTTER_CURSOR_DEVICE`.
    Cursor = 9,
    /// `CLUTTER_PAD_DEVICE`.
    Pad = 10,
    /// `CLUTTER_N_DEVICE_TYPES` — sentinel count, not a real device type.
    NDeviceTypes = 11,
}

/// `ClutterInputCapabilities` (clutter-enums.h) — a bitfield of device
/// capabilities. Values match the C bit positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct InputCapabilities(pub u32);

impl InputCapabilities {
    /// `CLUTTER_INPUT_CAPABILITY_POINTER`.
    pub const POINTER: Self = Self(1 << 0);
    /// `CLUTTER_INPUT_CAPABILITY_KEYBOARD`.
    pub const KEYBOARD: Self = Self(1 << 1);
    /// `CLUTTER_INPUT_CAPABILITY_TOUCHPAD`.
    pub const TOUCHPAD: Self = Self(1 << 2);
    /// `CLUTTER_INPUT_CAPABILITY_TOUCH`.
    pub const TOUCH: Self = Self(1 << 3);
    /// `CLUTTER_INPUT_CAPABILITY_TABLET_TOOL`.
    pub const TABLET_TOOL: Self = Self(1 << 4);
    /// `CLUTTER_INPUT_CAPABILITY_TABLET_PAD`.
    pub const TABLET_PAD: Self = Self(1 << 5);
    pub const NONE: Self = Self(0);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Port of `ClutterInputDevice` / `ClutterInputDevicePrivate`.
#[derive(Debug, Clone)]
pub struct InputDevice {
    pub device_type: InputDeviceType,
    pub capabilities: InputCapabilities,
    pub device_name: Option<String>,
    pub vendor_id: u32,
    pub product_id: u32,
    pub bus_type: u32,
    pub node_path: Option<String>,
    pub n_rings: i32,
    pub n_strips: i32,
    pub n_dials: i32,
    pub n_mode_groups: i32,
    pub n_buttons: i32,
}

impl Default for InputDevice {
    fn default() -> Self {
        // Matches `clutter_input_device_init`: `device_type = POINTER`.
        InputDevice {
            device_type: InputDeviceType::Pointer,
            capabilities: InputCapabilities::NONE,
            device_name: None,
            vendor_id: 0,
            product_id: 0,
            bus_type: 0,
            node_path: None,
            n_rings: 0,
            n_strips: 0,
            n_dials: 0,
            n_mode_groups: 0,
            n_buttons: 0,
        }
    }
}

impl InputDevice {
    /// Construct a device, running the `constructed` capability inference
    /// when `capabilities` is empty (matching `clutter_input_device_constructed`).
    pub fn new(
        device_type: InputDeviceType,
        capabilities: InputCapabilities,
        device_name: Option<String>,
        vendor_id: u32,
        product_id: u32,
        bus_type: u32,
        node_path: Option<String>,
        n_rings: i32,
        n_strips: i32,
        n_dials: i32,
        n_mode_groups: i32,
        n_buttons: i32,
    ) -> Self {
        let capabilities = if capabilities.is_empty() {
            Self::infer_capabilities(device_type)
        } else {
            capabilities
        };
        InputDevice {
            device_type,
            capabilities,
            device_name,
            vendor_id,
            product_id,
            bus_type,
            node_path,
            n_rings,
            n_strips,
            n_dials,
            n_mode_groups,
            n_buttons,
        }
    }

    /// Port of the capability-inference `switch` in
    /// `clutter_input_device_constructed`: when `capabilities` is empty,
    /// derive it from `device_type`.
    pub fn infer_capabilities(device_type: InputDeviceType) -> InputCapabilities {
        match device_type {
            InputDeviceType::Pointer => InputCapabilities::POINTER,
            InputDeviceType::Keyboard => InputCapabilities::KEYBOARD,
            InputDeviceType::Touchpad => {
                // POINTER | TOUCHPAD
                InputCapabilities(InputCapabilities::POINTER.0 | InputCapabilities::TOUCHPAD.0)
            }
            InputDeviceType::Touchscreen => InputCapabilities::TOUCH,
            InputDeviceType::Tablet
            | InputDeviceType::Pen
            | InputDeviceType::Eraser
            | InputDeviceType::Cursor => InputCapabilities::TABLET_TOOL,
            InputDeviceType::Pad => InputCapabilities::TABLET_PAD,
            InputDeviceType::Extension | InputDeviceType::Joystick => InputCapabilities::NONE,
            InputDeviceType::NDeviceTypes => InputCapabilities::NONE,
        }
    }

    // ---- getters mirroring clutter_input_device_get_* ----

    /// `clutter_input_device_get_device_type`.
    pub fn device_type(&self) -> InputDeviceType {
        self.device_type
    }
    /// `clutter_input_device_get_capabilities`.
    pub fn capabilities(&self) -> InputCapabilities {
        self.capabilities
    }
    /// `clutter_input_device_get_device_name`.
    pub fn device_name(&self) -> Option<&str> {
        self.device_name.as_deref()
    }
    /// `clutter_input_device_get_vendor_id`.
    pub fn vendor_id(&self) -> u32 {
        self.vendor_id
    }
    /// `clutter_input_device_get_product_id`.
    pub fn product_id(&self) -> u32 {
        self.product_id
    }
    /// `clutter_input_device_get_bus_type` (the property getter).
    pub fn bus_type(&self) -> u32 {
        self.bus_type
    }
    /// `clutter_input_device_get_node_path`.
    pub fn node_path(&self) -> Option<&str> {
        self.node_path.as_deref()
    }
    /// `clutter_input_device_get_n_rings`.
    pub fn n_rings(&self) -> i32 {
        self.n_rings
    }
    /// `clutter_input_device_get_n_strips`.
    pub fn n_strips(&self) -> i32 {
        self.n_strips
    }
    /// `clutter_input_device_get_n_dials`.
    pub fn n_dials(&self) -> i32 {
        self.n_dials
    }
    /// `clutter_input_device_get_n_mode_groups`.
    pub fn n_mode_groups(&self) -> i32 {
        self.n_mode_groups
    }
    /// `clutter_input_device_get_n_buttons`.
    pub fn n_buttons(&self) -> i32 {
        self.n_buttons
    }

    /// Human-readable debug label, mirroring the
    /// `_clutter_input_device_get_debug_name` fallback (name or
    /// "<device>" / type-based). Useful for logging without the GObject
    /// debug-name subsystem.
    pub fn debug_name(&self) -> String {
        self.device_name
            .clone()
            .unwrap_or_else(|| match self.device_type {
                InputDeviceType::Pointer => "pointer".to_string(),
                InputDeviceType::Keyboard => "keyboard".to_string(),
                InputDeviceType::Touchpad => "touchpad".to_string(),
                InputDeviceType::Touchscreen => "touchscreen".to_string(),
                InputDeviceType::Tablet => "tablet".to_string(),
                InputDeviceType::Pen => "pen".to_string(),
                InputDeviceType::Eraser => "eraser".to_string(),
                InputDeviceType::Cursor => "cursor".to_string(),
                InputDeviceType::Pad => "pad".to_string(),
                InputDeviceType::Joystick => "joystick".to_string(),
                InputDeviceType::Extension => "extension".to_string(),
                InputDeviceType::NDeviceTypes => "unknown".to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_pointer_no_caps() {
        let d = InputDevice::default();
        assert_eq!(d.device_type(), InputDeviceType::Pointer);
        assert!(d.capabilities().is_empty());
    }

    #[test]
    fn new_infers_capabilities_when_empty() {
        let d = InputDevice::new(
            InputDeviceType::Touchpad,
            InputCapabilities::NONE,
            None,
            0,
            0,
            0,
            None,
            0,
            0,
            0,
            0,
            0,
        );
        // Touchpad -> POINTER | TOUCHPAD.
        assert!(d.capabilities().contains(InputCapabilities::POINTER));
        assert!(d.capabilities().contains(InputCapabilities::TOUCHPAD));
    }

    #[test]
    fn new_keeps_explicit_capabilities() {
        let d = InputDevice::new(
            InputDeviceType::Pointer,
            InputCapabilities::KEYBOARD, // unusual but explicit
            None,
            0,
            0,
            0,
            None,
            0,
            0,
            0,
            0,
            0,
        );
        assert_eq!(d.capabilities(), InputCapabilities::KEYBOARD);
    }

    #[test]
    fn infer_capabilities_matches_constructed_switch() {
        assert_eq!(
            InputDevice::infer_capabilities(InputDeviceType::Pointer),
            InputCapabilities::POINTER
        );
        assert_eq!(
            InputDevice::infer_capabilities(InputDeviceType::Keyboard),
            InputCapabilities::KEYBOARD
        );
        assert_eq!(
            InputDevice::infer_capabilities(InputDeviceType::Touchscreen),
            InputCapabilities::TOUCH
        );
        assert_eq!(
            InputDevice::infer_capabilities(InputDeviceType::Pad),
            InputCapabilities::TABLET_PAD
        );
        let tab = InputDevice::infer_capabilities(InputDeviceType::Tablet);
        assert_eq!(tab, InputCapabilities::TABLET_TOOL);
        let tp = InputDevice::infer_capabilities(InputDeviceType::Touchpad);
        assert!(tp.contains(InputCapabilities::POINTER));
        assert!(tp.contains(InputCapabilities::TOUCHPAD));
    }

    #[test]
    fn debug_name_falls_back_to_type() {
        let d = InputDevice::default();
        assert_eq!(d.debug_name(), "pointer");
        let mut d = d;
        d.device_name = Some("Logitech MX".into());
        assert_eq!(d.debug_name(), "Logitech MX");
    }
}
