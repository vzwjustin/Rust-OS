//! MetaInputSettings ported from GNOME Mutter's src/core/meta-input-settings.c
//!
//! MetaInputSettings manages per-device input configuration: mouse/touchpad
//! acceleration, natural scroll, tap-to-click, keyboard repeat rates, tablet
//! mapping, and accessibility settings. In Mutter this reads from GSettings
//! schemas (org.gnome.desktop.peripherals, org.gnome.desktop.input-sources)
//! and applies them to ClutterInputDevice objects.
//!
//! In the kernel, GSettings and Clutter are not available. The settings are
//! stored as plain fields and applied via a trait-based device handle, so
//! callers can feed real device configuration and query the effective values.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-input-settings.c

use alloc::string::String;
use alloc::vec::Vec;

/// Device category for settings application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsDeviceCategory {
    Mouse,
    Touchpad,
    Trackball,
    Keyboard,
    Tablet,
    Touchscreen,
}

/// Mouse/touchpad/trackball speed (-1.0 = slowest, 1.0 = fastest).
/// Mirrors the org.gnome.desktop.peripherals.mouse speed setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerSpeed {
    pub accel_profile: AccelProfile,
    pub speed: f64,
    pub natural_scroll: bool,
    pub left_handed: bool,
}

impl Default for PointerSpeed {
    fn default() -> Self {
        PointerSpeed {
            accel_profile: AccelProfile::default(),
            speed: 0.0,
            natural_scroll: false,
            left_handed: false,
        }
    }
}

/// Acceleration profile, mirrors org.gnome.desktop.peripherals.mouse accel-profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelProfile {
    Flat,
    Adaptive,
    Default,
}

impl Default for AccelProfile {
    fn default() -> Self {
        AccelProfile::Default
    }
}

/// Touchpad-specific settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchpadSettings {
    pub tap_to_click: bool,
    pub natural_scroll: bool,
    pub two_finger_scroll: bool,
    pub edge_scroll: bool,
    pub disable_while_typing: bool,
    pub click_method: ClickMethod,
    pub send_events: SendEventsMode,
    pub speed: f64,
}

impl Default for TouchpadSettings {
    fn default() -> Self {
        TouchpadSettings {
            tap_to_click: false,
            natural_scroll: true,
            two_finger_scroll: true,
            edge_scroll: false,
            disable_while_typing: true,
            click_method: ClickMethod::default(),
            send_events: SendEventsMode::default(),
            speed: 0.0,
        }
    }
}

/// Touchpad click method, mirrors org.gnome.desktop.peripherals.touchpad click-method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickMethod {
    Default,
    Areas,
    Fingers,
}

impl Default for ClickMethod {
    fn default() -> Self {
        ClickMethod::Default
    }
}

/// When to send events from a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendEventsMode {
    Enabled,
    Disabled,
    DisabledOnExternalMouse,
    DisabledOnExternalKeyboard,
}

impl Default for SendEventsMode {
    fn default() -> Self {
        SendEventsMode::Enabled
    }
}

/// Keyboard settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardSettings {
    pub repeat: bool,
    /// Delay before repeat starts, in milliseconds.
    pub repeat_delay_ms: u32,
    /// Repeat interval, in milliseconds.
    pub repeat_interval_ms: u32,
    /// Current keyboard layout index.
    pub layout_index: u32,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        KeyboardSettings {
            repeat: true,
            repeat_delay_ms: 500,
            repeat_interval_ms: 33,
            layout_index: 0,
        }
    }
}

/// Tablet settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabletSettings {
    pub mapping: TabletMapping,
    pub area: TabletArea,
    pub keep_aspect: bool,
    pub left_handed: bool,
}

impl Default for TabletSettings {
    fn default() -> Self {
        TabletSettings {
            mapping: TabletMapping::default(),
            area: TabletArea::default(),
            keep_aspect: false,
            left_handed: false,
        }
    }
}

/// Tablet mapping mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabletMapping {
    Absolute,
    Relative,
}

impl Default for TabletMapping {
    fn default() -> Self {
        TabletMapping::Absolute
    }
}

/// Tablet area mapping (which portion of the tablet maps to the screen).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TabletArea {
    /// Map the entire tablet surface.
    Full,
    /// Map a sub-region [0..1]×[0..1] of the tablet.
    Partial {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
}

impl Default for TabletArea {
    fn default() -> Self {
        TabletArea::Full
    }
}

/// A device handle for applying settings. The backend implements this to
/// push settings into the actual hardware/libinput layer.
pub trait InputDeviceHandle {
    /// Device category.
    fn category(&self) -> SettingsDeviceCategory;
    /// Set pointer speed/acceleration.
    fn set_speed(&mut self, speed: f64);
    /// Set natural scroll direction.
    fn set_natural_scroll(&mut self, enabled: bool);
    /// Set left-handed mode.
    fn set_left_handed(&mut self, enabled: bool);
    /// Set tap-to-click (touchpad).
    fn set_tap_to_click(&mut self, enabled: bool);
    /// Set the send-events mode.
    fn set_send_events(&mut self, mode: SendEventsMode);
}

/// The input settings manager. Mirrors MetaInputSettings.
#[derive(Debug)]
pub struct MetaInputSettings {
    mouse: PointerSpeed,
    touchpad: TouchpadSettings,
    keyboard: KeyboardSettings,
    tablet: TabletSettings,
    /// Number of registered devices (for tracking; actual devices are
    /// managed by the backend).
    device_count: usize,
}

impl MetaInputSettings {
    /// Create a new input settings manager with defaults. Mirrors
    /// meta_input_settings_new().
    pub fn new() -> Self {
        MetaInputSettings {
            mouse: PointerSpeed::default(),
            touchpad: TouchpadSettings::default(),
            keyboard: KeyboardSettings::default(),
            tablet: TabletSettings::default(),
            device_count: 0,
        }
    }

    // ── Mouse settings ────────────────────────────────────────────────

    pub fn mouse(&self) -> &PointerSpeed {
        &self.mouse
    }

    pub fn mouse_mut(&mut self) -> &mut PointerSpeed {
        &mut self.mouse
    }

    pub fn set_mouse_speed(&mut self, speed: f64) {
        self.mouse.speed = speed.clamp(-1.0, 1.0);
    }

    pub fn set_mouse_natural_scroll(&mut self, enabled: bool) {
        self.mouse.natural_scroll = enabled;
    }

    pub fn set_mouse_left_handed(&mut self, enabled: bool) {
        self.mouse.left_handed = enabled;
    }

    pub fn set_accel_profile(&mut self, profile: AccelProfile) {
        self.mouse.accel_profile = profile;
    }

    // ── Touchpad settings ─────────────────────────────────────────────

    pub fn touchpad(&self) -> &TouchpadSettings {
        &self.touchpad
    }

    pub fn touchpad_mut(&mut self) -> &mut TouchpadSettings {
        &mut self.touchpad
    }

    pub fn set_tap_to_click(&mut self, enabled: bool) {
        self.touchpad.tap_to_click = enabled;
    }

    pub fn set_touchpad_natural_scroll(&mut self, enabled: bool) {
        self.touchpad.natural_scroll = enabled;
    }

    pub fn set_two_finger_scroll(&mut self, enabled: bool) {
        self.touchpad.two_finger_scroll = enabled;
    }

    pub fn set_disable_while_typing(&mut self, enabled: bool) {
        self.touchpad.disable_while_typing = enabled;
    }

    pub fn set_touchpad_speed(&mut self, speed: f64) {
        self.touchpad.speed = speed.clamp(-1.0, 1.0);
    }

    // ── Keyboard settings ─────────────────────────────────────────────

    pub fn keyboard(&self) -> &KeyboardSettings {
        &self.keyboard
    }

    pub fn keyboard_mut(&mut self) -> &mut KeyboardSettings {
        &mut self.keyboard
    }

    pub fn set_keyboard_repeat(&mut self, repeat: bool) {
        self.keyboard.repeat = repeat;
    }

    pub fn set_keyboard_repeat_rate(&mut self, delay_ms: u32, interval_ms: u32) {
        self.keyboard.repeat_delay_ms = delay_ms;
        self.keyboard.repeat_interval_ms = interval_ms;
    }

    pub fn set_keyboard_layout(&mut self, index: u32) {
        self.keyboard.layout_index = index;
    }

    // ── Tablet settings ───────────────────────────────────────────────

    pub fn tablet(&self) -> &TabletSettings {
        &self.tablet
    }

    pub fn tablet_mut(&mut self) -> &mut TabletSettings {
        &mut self.tablet
    }

    pub fn set_tablet_mapping(&mut self, mapping: TabletMapping) {
        self.tablet.mapping = mapping;
    }

    pub fn set_tablet_left_handed(&mut self, enabled: bool) {
        self.tablet.left_handed = enabled;
    }

    // ── Device tracking ───────────────────────────────────────────────

    pub fn device_count(&self) -> usize {
        self.device_count
    }

    pub fn set_device_count(&mut self, count: usize) {
        self.device_count = count;
    }

    // ── Apply settings to a device ────────────────────────────────────

    /// Apply the current settings to a device handle. Mirrors
    /// meta_input_settings_apply_settings_to_device().
    pub fn apply_to_device(&self, device: &mut dyn InputDeviceHandle) {
        match device.category() {
            SettingsDeviceCategory::Mouse | SettingsDeviceCategory::Trackball => {
                device.set_speed(self.mouse.speed);
                device.set_natural_scroll(self.mouse.natural_scroll);
                device.set_left_handed(self.mouse.left_handed);
            }
            SettingsDeviceCategory::Touchpad => {
                device.set_speed(self.touchpad.speed);
                device.set_natural_scroll(self.touchpad.natural_scroll);
                device.set_tap_to_click(self.touchpad.tap_to_click);
                device.set_send_events(self.touchpad.send_events);
            }
            SettingsDeviceCategory::Keyboard => {
                // Keyboard settings are applied via the keyboard repeat
                // rate, not through the InputDeviceHandle trait.
            }
            SettingsDeviceCategory::Tablet => {
                device.set_left_handed(self.tablet.left_handed);
            }
            SettingsDeviceCategory::Touchscreen => {
                // Touchscreens don't have configurable settings here.
            }
        }
    }
}

impl Default for MetaInputSettings {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let settings = MetaInputSettings::new();
        assert_eq!(settings.mouse().speed, 0.0);
        assert!(!settings.mouse().natural_scroll);
        assert!(settings.touchpad().natural_scroll);
        assert!(settings.touchpad().two_finger_scroll);
        assert!(settings.keyboard().repeat);
        assert_eq!(settings.keyboard().repeat_delay_ms, 500);
    }

    #[test]
    fn test_set_mouse_speed() {
        let mut settings = MetaInputSettings::new();
        settings.set_mouse_speed(0.5);
        assert_eq!(settings.mouse().speed, 0.5);

        settings.set_mouse_speed(2.0);
        assert_eq!(settings.mouse().speed, 1.0); // clamped

        settings.set_mouse_speed(-2.0);
        assert_eq!(settings.mouse().speed, -1.0); // clamped
    }

    #[test]
    fn test_touchpad_settings() {
        let mut settings = MetaInputSettings::new();
        settings.set_tap_to_click(true);
        settings.set_touchpad_natural_scroll(false);
        settings.set_two_finger_scroll(false);

        assert!(settings.touchpad().tap_to_click);
        assert!(!settings.touchpad().natural_scroll);
        assert!(!settings.touchpad().two_finger_scroll);
    }

    #[test]
    fn test_keyboard_repeat() {
        let mut settings = MetaInputSettings::new();
        settings.set_keyboard_repeat(false);
        assert!(!settings.keyboard().repeat);

        settings.set_keyboard_repeat_rate(300, 25);
        assert_eq!(settings.keyboard().repeat_delay_ms, 300);
        assert_eq!(settings.keyboard().repeat_interval_ms, 25);
    }

    #[test]
    fn test_tablet_mapping() {
        let mut settings = MetaInputSettings::new();
        settings.set_tablet_mapping(TabletMapping::Relative);
        assert_eq!(settings.tablet().mapping, TabletMapping::Relative);
    }

    #[test]
    fn test_apply_to_device() {
        struct TestMouse {
            speed: f64,
            natural_scroll: bool,
            left_handed: bool,
            tap_to_click: bool,
            send_events: SendEventsMode,
        }

        impl InputDeviceHandle for TestMouse {
            fn category(&self) -> SettingsDeviceCategory {
                SettingsDeviceCategory::Mouse
            }
            fn set_speed(&mut self, speed: f64) {
                self.speed = speed;
            }
            fn set_natural_scroll(&mut self, enabled: bool) {
                self.natural_scroll = enabled;
            }
            fn set_left_handed(&mut self, enabled: bool) {
                self.left_handed = enabled;
            }
            fn set_tap_to_click(&mut self, enabled: bool) {
                self.tap_to_click = enabled;
            }
            fn set_send_events(&mut self, mode: SendEventsMode) {
                self.send_events = mode;
            }
        }

        let mut settings = MetaInputSettings::new();
        settings.set_mouse_speed(0.7);
        settings.set_mouse_natural_scroll(true);
        settings.set_mouse_left_handed(true);

        let mut mouse = TestMouse {
            speed: 0.0,
            natural_scroll: false,
            left_handed: false,
            tap_to_click: false,
            send_events: SendEventsMode::Enabled,
        };

        settings.apply_to_device(&mut mouse);
        assert_eq!(mouse.speed, 0.7);
        assert!(mouse.natural_scroll);
        assert!(mouse.left_handed);
    }
}
