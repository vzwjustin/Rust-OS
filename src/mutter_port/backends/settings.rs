//! GNOME Mutter's src/backends/meta-settings.c
//!
//! Backend-wide settings: UI/global scaling factors, font DPI, privacy screen,
//! experimental features, and Xwayland grab/scaling knobs. In Mutter these are
//! backed by several GSettings schemas and pushed into Clutter; here the values
//! and their derivation logic are kept, with the GSettings/Clutter/monitor
//! plumbing stubbed.
//!
//! Stubbed: GSettings schemas (org.gnome.desktop.interface, .privacy,
//! org.gnome.mutter, .wayland), GObject signals, ClutterSettings font-dpi push,
//! and the MetaMonitorManager scale lookup. Callers feed inputs via setters
//! (e.g. `set_global_scaling_factor`, `set_primary_monitor_scale`); derived
//! values (ui scaling, font dpi) are recomputed the same way as the C.
//!
//! Reference:
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-settings.c

use alloc::string::String;
use alloc::vec::Vec;

/// MetaExperimentalFeature bitflags (the subset defined in the mutter schema
/// handling). Kept as a bitmask on the settings struct.
pub mod experimental_feature {
    pub const NONE: u32 = 0;
    pub const KMS_MODIFIERS: u32 = 1 << 0;
    pub const AUTOCLOSE_XWAYLAND: u32 = 1 << 1;
}

/// Change signals, replacing the GObject "…-changed" signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSignal {
    UiScalingFactorChanged,
    GlobalScalingFactorChanged,
    FontDpiChanged,
    ExperimentalFeaturesChanged,
    PrivacyScreenChanged,
    XwaylandScalingFactorChanged,
}

/// MetaSettings. Mirrors struct _MetaSettings (minus the GSettings/backend
/// object handles).
#[derive(Debug)]
pub struct Settings {
    ui_scaling_factor: i32,
    global_scaling_factor: i32,
    font_dpi: i32,
    privacy_screen: bool,

    experimental_features: u32,
    experimental_features_overridden: bool,

    xwayland_allow_grabs: bool,
    xwayland_grab_allow_list_patterns: Vec<String>,
    xwayland_grab_deny_list_patterns: Vec<String>,
    xwayland_disable_extensions: i32,
    xwayland_allow_byte_swapped_clients: bool,
    xwayland_scaling_factor: f32,

    // Stubbed inputs that would come from the monitor manager / stage-views.
    primary_monitor_scale: i32,
    stage_views_scaled: bool,
    text_scaling_factor: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

impl Settings {
    /// meta_settings_new() / meta_settings_init(): defaults roughly matching a
    /// fresh unscaled desktop.
    pub fn new() -> Self {
        Settings {
            ui_scaling_factor: 1,
            global_scaling_factor: 0,
            font_dpi: 0,
            privacy_screen: false,
            experimental_features: experimental_feature::NONE,
            experimental_features_overridden: false,
            xwayland_allow_grabs: false,
            xwayland_grab_allow_list_patterns: Vec::new(),
            xwayland_grab_deny_list_patterns: Vec::new(),
            xwayland_disable_extensions: 0,
            xwayland_allow_byte_swapped_clients: false,
            xwayland_scaling_factor: 0.0,
            primary_monitor_scale: 1,
            stage_views_scaled: false,
            text_scaling_factor: 1.0,
        }
    }

    // --- UI scaling -------------------------------------------------------

    /// calculate_ui_scaling_factor(): from the primary logical monitor scale.
    fn calculate_ui_scaling_factor(&self) -> i32 {
        if self.primary_monitor_scale <= 0 {
            1
        } else {
            self.primary_monitor_scale
        }
    }

    /// update_ui_scaling_factor(): returns true if the value changed.
    fn update_ui_scaling_factor(&mut self) -> bool {
        let ui = if self.stage_views_scaled {
            1
        } else {
            self.calculate_ui_scaling_factor()
        };
        if self.ui_scaling_factor != ui {
            self.ui_scaling_factor = ui;
            true
        } else {
            false
        }
    }

    /// meta_settings_update_ui_scaling_factor()
    pub fn update_ui_scaling_factor_signal(&mut self) -> Option<SettingsSignal> {
        if self.update_ui_scaling_factor() {
            Some(SettingsSignal::UiScalingFactorChanged)
        } else {
            None
        }
    }

    /// meta_settings_get_ui_scaling_factor()
    pub fn ui_scaling_factor(&self) -> i32 {
        self.ui_scaling_factor
    }

    /// Feed the primary logical monitor scale (from the monitor manager stub).
    pub fn set_primary_monitor_scale(&mut self, scale: i32) {
        self.primary_monitor_scale = scale;
    }

    /// Whether stage views are scaled (meta_backend_is_stage_views_scaled stub).
    pub fn set_stage_views_scaled(&mut self, scaled: bool) {
        self.stage_views_scaled = scaled;
    }

    // --- Global scaling ---------------------------------------------------

    /// update_global_scaling_factor() via the interface "scaling-factor" key.
    pub fn set_global_scaling_factor(&mut self, factor: i32) -> Option<SettingsSignal> {
        if self.global_scaling_factor != factor {
            self.global_scaling_factor = factor;
            Some(SettingsSignal::GlobalScalingFactorChanged)
        } else {
            None
        }
    }

    /// meta_settings_get_global_scaling_factor()
    pub fn global_scaling_factor(&self) -> Option<i32> {
        if self.global_scaling_factor == 0 {
            None
        } else {
            Some(self.global_scaling_factor)
        }
    }

    // --- Font DPI ---------------------------------------------------------

    /// update_font_dpi(): DPI = text_scaling * 96 * 1024 * ui_scaling_factor.
    fn update_font_dpi(&mut self) -> bool {
        const DOTS_PER_INCH: f64 = 96.0;
        const XFT_FACTOR: f64 = 1024.0;
        let font_dpi =
            (self.text_scaling_factor * DOTS_PER_INCH * XFT_FACTOR * self.ui_scaling_factor as f64)
                as i32;
        if font_dpi != self.font_dpi {
            self.font_dpi = font_dpi;
            // Would push "font-dpi" into ClutterSettings here.
            true
        } else {
            false
        }
    }

    /// meta_settings_update_font_dpi()
    pub fn update_font_dpi_signal(&mut self) -> Option<SettingsSignal> {
        if self.update_font_dpi() {
            Some(SettingsSignal::FontDpiChanged)
        } else {
            None
        }
    }

    /// meta_settings_get_font_dpi()
    pub fn font_dpi(&self) -> i32 {
        self.font_dpi
    }

    /// Feed the "text-scaling-factor" interface gsetting.
    pub fn set_text_scaling_factor(&mut self, factor: f64) {
        self.text_scaling_factor = factor;
    }

    // --- Experimental features -------------------------------------------

    /// meta_settings_is_experimental_feature_enabled()
    pub fn is_experimental_feature_enabled(&self, feature: u32) -> bool {
        self.experimental_features & feature != 0
    }

    /// meta_settings_override_experimental_features()
    pub fn override_experimental_features(&mut self) {
        self.experimental_features = experimental_feature::NONE;
        self.experimental_features_overridden = true;
    }

    /// meta_settings_enable_experimental_feature()
    pub fn enable_experimental_feature(&mut self, feature: u32) {
        debug_assert!(self.experimental_features_overridden);
        self.experimental_features |= feature;
    }

    /// experimental_features_handler() + update_experimental_features():
    /// parse a list of feature-name strings from the "experimental-features"
    /// gsetting, returning the change signal (with the old bitmask) if changed.
    pub fn set_experimental_features_from_strings(
        &mut self,
        features: &[&str],
    ) -> Option<(SettingsSignal, u32)> {
        if self.experimental_features_overridden {
            return None;
        }
        let old = self.experimental_features;
        let mut new = experimental_feature::NONE;
        for f in features {
            new |= match *f {
                "kms-modifiers" => experimental_feature::KMS_MODIFIERS,
                "autoclose-xwayland" => experimental_feature::AUTOCLOSE_XWAYLAND,
                _ => experimental_feature::NONE, // unknown feature: warn + ignore
            };
        }
        if new != self.experimental_features {
            self.experimental_features = new;
            Some((SettingsSignal::ExperimentalFeaturesChanged, old))
        } else {
            None
        }
    }

    // --- Privacy screen ---------------------------------------------------

    /// meta_settings_is_privacy_screen_enabled()
    pub fn is_privacy_screen_enabled(&self) -> bool {
        self.privacy_screen
    }

    /// privacy_settings_changed(): react to the "privacy-screen" gsetting.
    pub fn update_privacy_screen(&mut self, enabled: bool) -> Option<SettingsSignal> {
        if self.privacy_screen != enabled {
            self.privacy_screen = enabled;
            Some(SettingsSignal::PrivacyScreenChanged)
        } else {
            None
        }
    }

    /// meta_settings_set_privacy_screen_enabled(): would also write the
    /// gsetting back. No signal is emitted on this path in the C.
    pub fn set_privacy_screen_enabled(&mut self, enabled: bool) {
        if self.privacy_screen == enabled {
            return;
        }
        self.privacy_screen = enabled;
        // Would g_settings_set_boolean("privacy-screen", enabled) here.
    }

    // --- Xwayland ---------------------------------------------------------

    /// xwayland_grab_list_add_item(): items prefixed with '!' are denials.
    pub fn add_xwayland_grab_item(&mut self, item: &str) {
        if let Some(rest) = item.strip_prefix('!') {
            if !rest.is_empty() {
                self.xwayland_grab_deny_list_patterns
                    .push(String::from(rest));
            }
        } else {
            self.xwayland_grab_allow_list_patterns
                .push(String::from(item));
        }
    }

    /// update_xwayland_grab_access_rules(): reset the pattern lists and repopulate
    /// from the given rules (system defaults followed by gsettings values).
    pub fn update_xwayland_grab_access_rules(&mut self, rules: &[&str]) {
        self.xwayland_grab_allow_list_patterns.clear();
        self.xwayland_grab_deny_list_patterns.clear();
        for rule in rules {
            self.add_xwayland_grab_item(rule);
        }
    }

    /// meta_settings_get_xwayland_grab_patterns()
    pub fn xwayland_grab_patterns(&self) -> (&[String], &[String]) {
        (
            &self.xwayland_grab_allow_list_patterns,
            &self.xwayland_grab_deny_list_patterns,
        )
    }

    /// update_xwayland_allow_grabs()
    pub fn set_xwayland_allow_grabs(&mut self, allow: bool) {
        self.xwayland_allow_grabs = allow;
    }

    /// meta_settings_are_xwayland_grabs_allowed()
    pub fn are_xwayland_grabs_allowed(&self) -> bool {
        self.xwayland_allow_grabs
    }

    /// update_xwayland_disable_extensions()
    pub fn set_xwayland_disable_extensions(&mut self, mask: i32) {
        self.xwayland_disable_extensions = mask;
    }

    /// meta_settings_get_xwayland_disable_extensions()
    pub fn xwayland_disable_extensions(&self) -> i32 {
        self.xwayland_disable_extensions
    }

    /// update_xwayland_allow_byte_swapped_clients()
    pub fn set_xwayland_allow_byte_swapped_clients(&mut self, allow: bool) {
        self.xwayland_allow_byte_swapped_clients = allow;
    }

    /// meta_settings_are_xwayland_byte_swapped_clients_allowed()
    pub fn are_xwayland_byte_swapped_clients_allowed(&self) -> bool {
        self.xwayland_allow_byte_swapped_clients
    }

    /// update_xwayland_scaling_factor()
    pub fn set_xwayland_scaling_factor(&mut self, factor: f32) {
        self.xwayland_scaling_factor = factor;
    }

    /// meta_settings_get_xwayland_scaling_factor(): None when ~0.
    pub fn xwayland_scaling_factor(&self) -> Option<f32> {
        if self.xwayland_scaling_factor.abs() < f32::EPSILON {
            None
        } else {
            Some(self.xwayland_scaling_factor)
        }
    }
}
