//! Input Settings Private — ported from GNOME Mutter
//!
//! Private virtual methods and internal types for input device settings.
//! Defines the backend-specific configuration methods for different device types.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-settings-private.h

use super::input_settings::{InputSettings, MetaKbdA11ySettings};

/// Virtual method table for input settings backends.
pub struct InputSettingsClass {
    // Virtual methods for device configuration
    // set_send_events, set_matrix, set_speed, set_left_handed, etc.
}

/// Device mapping information for input settings.
pub struct DeviceMappingInfo {
    // device reference
    // settings reference
    // group_modes
    // aspect_ratio
}

/// Tool-specific settings information.
pub struct CurrentToolInfo {
    // input_settings reference
    // device reference
    // tool reference
    // settings reference
}
