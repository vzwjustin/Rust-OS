//! Miscellaneous Mutter types
//! Ported from various meta/*.h files
use alloc::{string::String, vec::Vec, format, collections::BTreeMap};

use crate::mutter_port::meta::types::*;

/// Drag and Drop manager. Tracks the state of drag-and-drop operations.
pub struct MetaDnd {
    dragging: bool,
    drag_data: Option<*mut core::ffi::c_void>,
}

impl MetaDnd {
    /// Begin DND operation
    pub fn begin_drag(&mut self) {
        self.dragging = true;
    }

    /// End DND operation
    pub fn end_drag(&mut self) {
        self.dragging = false;
        self.drag_data = None;
    }

    /// Check if DND in progress
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }
}

impl Default for MetaDnd {
    fn default() -> Self {
        Self {
            dragging: false,
            drag_data: None,
        }
    }
}

/// Settings/preferences. Stores configuration key-value pairs.
pub struct MetaSettings {
    settings: BTreeMap<String, String>,
}

impl MetaSettings {
    /// Get setting value as bool
    pub fn get_bool(&self, key: &str) -> bool {
        self.settings
            .get(key)
            .map(|v| v == "true")
            .unwrap_or(false)
    }

    /// Set setting value
    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.settings.insert(
            String::from(key),
            if value { "true" } else { "false" }.into(),
        );
    }

    /// Get setting value as string
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.settings.get(key).cloned()
    }
}

impl Default for MetaSettings {
    fn default() -> Self {
        Self {
            settings: BTreeMap::new(),
        }
    }
}

/// Window configuration constraints. Stores size and aspect ratio hints.
pub struct MetaWindowConfig {
    width: Option<i32>,
    height: Option<i32>,
    aspect_ratio: Option<f32>,
}

impl MetaWindowConfig {
    /// Get width hint
    pub fn get_width(&self) -> Option<i32> {
        self.width
    }

    /// Get height hint
    pub fn get_height(&self) -> Option<i32> {
        self.height
    }

    /// Get aspect ratio
    pub fn get_aspect_ratio(&self) -> Option<f32> {
        self.aspect_ratio
    }
}

impl Default for MetaWindowConfig {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            aspect_ratio: None,
        }
    }
}

/// External constraint on window positioning. Represents an active constraint.
pub struct MetaExternalConstraint {
    active: bool,
}

impl MetaExternalConstraint {
    /// Check if constraint is active
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Default for MetaExternalConstraint {
    fn default() -> Self {
        Self { active: false }
    }
}

/// Screen backlight control. Manages display brightness.
pub struct MetaBacklight {
    brightness: u32,
    max_brightness: u32,
}

impl MetaBacklight {
    /// Get current brightness (0-100)
    pub fn get_brightness(&self) -> u32 {
        self.brightness
    }

    /// Set brightness
    pub fn set_brightness(&mut self, level: u32) {
        self.brightness = level.min(self.max_brightness);
    }

    /// Get max brightness value
    pub fn get_max_brightness(&self) -> u32 {
        self.max_brightness
    }
}

impl Default for MetaBacklight {
    fn default() -> Self {
        Self {
            brightness: 100,
            max_brightness: 100,
        }
    }
}

/// Dialog for close confirmation. Manages visibility of close confirmation dialog.
pub struct MetaCloseDialog {
    visible: bool,
}

impl MetaCloseDialog {
    /// Show close confirmation dialog
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide close dialog
    pub fn hide(&mut self) {
        self.visible = false;
    }
}

impl Default for MetaCloseDialog {
    fn default() -> Self {
        Self { visible: false }
    }
}

/// Application launch context. Context for launching applications.
pub struct MetaLaunchContext {
    display_name: Option<String>,
}

impl MetaLaunchContext {
    pub fn new() -> Self {
        Self {
            display_name: None,
        }
    }
}

impl Default for MetaLaunchContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Sound player for system sounds. Plays system notification sounds.
pub struct MetaSoundPlayer {
    current_sound: Option<String>,
}

impl MetaSoundPlayer {
    pub fn new() -> Self {
        Self {
            current_sound: None,
        }
    }

    /// Play system sound
    pub fn play_from_file(&self, _path: &str, _display_name: &str) {
        // TODO: implement
    }

    /// Stop playing sound
    pub fn stop(&self) {
        // TODO: implement
    }
}

impl Default for MetaSoundPlayer {
    fn default() -> Self {
        Self::new()
    }
}
