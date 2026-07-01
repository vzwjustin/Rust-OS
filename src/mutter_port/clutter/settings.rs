//! ClutterSettings: global per-application UI settings singleton.
//!
//! Ported from GNOME mutter's clutter-settings.c/.h.
//! No GSettings/dconf backend; in-memory struct only with upstream defaults.

/// UI settings: double-click timing, drag thresholds, fonts, accessibility.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Double-click time in milliseconds (default: 250).
    pub double_click_time: i32,
    /// Double-click distance in pixels (default: 5).
    pub double_click_distance: i32,
    /// Drag threshold distance in pixels (default: 8).
    pub dnd_drag_threshold: i32,
    /// Font DPI; -1 means auto-detect (default: -1).
    pub font_dpi: i32,
    /// Font resolution in DPI; -1 means use font_dpi (default: -1.0).
    pub resolution: f64,
    /// Long-press gesture duration in milliseconds (default: 500).
    pub long_press_duration: i32,
    /// Password field hint visibility time in milliseconds (default: 0 = disabled).
    pub password_hint_time: u32,
    /// Font name/description; default is "Sans 12" via const.
    pub font_name: Option<&'static str>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            double_click_time: 250,
            double_click_distance: 5,
            dnd_drag_threshold: 8,
            font_dpi: -1,
            resolution: -1.0,
            long_press_duration: 500,
            password_hint_time: 0,
            font_name: Some("Sans 12"),
        }
    }
}

impl Settings {
    /// Create new settings with upstream defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set double-click time (ms).
    pub fn set_double_click_time(&mut self, ms: i32) {
        self.double_click_time = ms;
    }

    /// Set double-click distance (pixels).
    pub fn set_double_click_distance(&mut self, pixels: i32) {
        self.double_click_distance = pixels;
    }

    /// Set DND drag threshold (pixels).
    pub fn set_dnd_drag_threshold(&mut self, pixels: i32) {
        self.dnd_drag_threshold = pixels;
    }

    /// Set font DPI; -1 = auto-detect.
    pub fn set_font_dpi(&mut self, dpi: i32) {
        self.font_dpi = dpi;
    }

    /// Set resolution in DPI; -1 = use font_dpi.
    pub fn set_resolution(&mut self, dpi: f64) {
        self.resolution = dpi;
    }

    /// Set long-press duration (ms).
    pub fn set_long_press_duration(&mut self, ms: i32) {
        self.long_press_duration = ms;
    }

    /// Set password hint visibility time (ms); 0 = disabled.
    pub fn set_password_hint_time(&mut self, ms: u32) {
        self.password_hint_time = ms;
    }

    /// Set font name/description.
    pub fn set_font_name(&mut self, name: Option<&'static str>) {
        self.font_name = name;
    }

    /// Get default font name.
    pub fn default_font_name() -> &'static str {
        "Sans 12"
    }
}
